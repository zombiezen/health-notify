// Copyright 2024 Ross Light
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use std::ffi::{OsStr, OsString};
use std::process::{self, Child, Command};
use std::time::Duration;

use anyhow::Result;
use clap::{
    value_parser, Arg, ArgAction, ArgMatches, CommandFactory, FromArgMatches, Parser, ValueHint,
};
use nix::sys::signal::kill;
use nix::unistd::Pid;
use signal_hook::{
    consts::{SIGCHLD, SIGHUP, SIGINT, SIGTERM, SIGUSR1, SIGUSR2},
    iterator::{exfiltrator::WithOrigin, SignalsInfo},
};

mod lazy_fail_init;
mod sd_notify;

#[derive(Clone, Debug)]
struct Options {
    child_notify: bool,
    child_argv: Vec<OsString>,
    check_argv: Vec<OsString>,
}

impl CommandFactory for Options {
    fn command() -> clap::Command {
        clap::Command::new("health-notify")
            .override_usage(
                "health-notify [options] CHILD_PROGRAM [ARG [...]] \\; CHECK_PROGRAM [ARG [...]]",
            )
            .arg(
                Arg::new("child_notify")
                    .help("Pass NOTIFY_SOCKET environment variable to child program")
                    .long("child-notify")
                    .action(ArgAction::SetTrue),
            )
            .arg(
                Arg::new("child_argv")
                    .help("Child program to run")
                    .action(ArgAction::Set)
                    .num_args(1..)
                    .value_terminator(";")
                    .required(true)
                    .allow_hyphen_values(true)
                    .value_parser(value_parser!(OsString)),
            )
            .arg(
                Arg::new("check_argv")
                    .help("Health checking program to run during startup")
                    .action(ArgAction::Set)
                    .num_args(1..)
                    .required(true)
                    .allow_hyphen_values(true)
                    .trailing_var_arg(true)
                    .value_parser(value_parser!(OsString))
                    .value_hint(ValueHint::CommandWithArguments),
            )
    }

    fn command_for_update() -> clap::Command {
        Self::command()
    }
}

impl FromArgMatches for Options {
    fn from_arg_matches(matches: &ArgMatches) -> Result<Self, clap::Error> {
        let mut matches = matches.clone();
        Self::from_arg_matches_mut(&mut matches)
    }

    fn from_arg_matches_mut(matches: &mut ArgMatches) -> Result<Self, clap::Error> {
        let mut opts = Options {
            child_notify: false,
            child_argv: Vec::new(),
            check_argv: Vec::new(),
        };
        opts.update_from_arg_matches_mut(matches)?;
        Ok(opts)
    }

    fn update_from_arg_matches(&mut self, matches: &ArgMatches) -> Result<(), clap::Error> {
        let mut matches = matches.clone();
        self.update_from_arg_matches_mut(&mut matches)
    }

    fn update_from_arg_matches_mut(&mut self, matches: &mut ArgMatches) -> Result<(), clap::Error> {
        self.child_notify = matches.get_flag("child_notify");
        self.child_argv = matches
            .remove_many::<OsString>("child_argv")
            .expect("child_argv is required")
            .collect();
        self.check_argv = matches
            .remove_many::<OsString>("check_argv")
            .expect("check_argv is required")
            .collect();
        Ok(())
    }
}

impl Parser for Options {}

fn main() -> Result<()> {
    let options = Options::parse();

    let mut signals =
        SignalsInfo::<WithOrigin>::new(&[SIGINT, SIGTERM, SIGUSR1, SIGUSR2, SIGHUP, SIGCHLD])?;
    let notify = if options.child_notify {
        sd_notify::SystemdNotify::from_env()
    } else {
        unsafe { sd_notify::SystemdNotify::take_from_env() }
    };

    let mut child = Command::new(&options.child_argv[0])
        .args(&options.child_argv[1..])
        .spawn()?;

    if let Err(exit_code) = wait_for_startup(&mut child, &options.check_argv, &mut signals) {
        process::exit(exit_code);
    }
    if let Some(notify) = notify {
        let _ = notify.notify("READY=1");
    }
    process::exit(propagate_signals(&mut child, &mut signals));
}

fn wait_for_startup<A: AsRef<OsStr>>(
    child: &mut Child,
    check_argv: &[A],
    signals: &mut SignalsInfo<WithOrigin>,
) -> Result<(), i32> {
    // Wait for some period of time then start a check subprocess.
    // We may get interrupted by signals or the check subprocess may fail to start,
    // so this can loop.
    'waitLoop: loop {
        let mut sleep_time = Duration::from_secs(1);
        let mut check_child = loop {
            match shuteye::sleep(sleep_time) {
                None => {
                    let spawn_result = Command::new(check_argv[0].as_ref())
                        .args(check_argv.iter().skip(1).map(AsRef::as_ref))
                        .env_remove(sd_notify::ENV_VAR)
                        .spawn();
                    if let Ok(check_child) = spawn_result {
                        break check_child;
                    } else {
                        continue 'waitLoop;
                    }
                }
                Some(remaining) => {
                    for sig in signals.pending() {
                        match sig.signal {
                            SIGCHLD => {
                                if sig.process.and_then(|p| u32::try_from(p.pid).ok())
                                    == Some(child.id())
                                {
                                    let exit_code = child
                                        .wait()
                                        .ok()
                                        .and_then(|status| status.code())
                                        .unwrap_or(1);
                                    return Err(exit_code);
                                }
                            }
                            _ => {
                                let _ = kill(
                                    Pid::from_raw(child.id().try_into().unwrap()),
                                    nix::sys::signal::Signal::try_from(sig.signal).unwrap(),
                                );
                            }
                        }
                    }

                    sleep_time = remaining;
                }
            }
        };

        // Now we're waiting for either process to exit.
        'checkLoop: loop {
            for sig in signals.wait() {
                match sig.signal {
                    SIGCHLD => {
                        if let Some(sig_pid) = sig.process.and_then(|p| u32::try_from(p.pid).ok()) {
                            if sig_pid == child.id() {
                                let exit_code = child
                                    .wait()
                                    .ok()
                                    .and_then(|status| status.code())
                                    .unwrap_or(1);
                                let _ = kill(
                                    Pid::from_raw(check_child.id().try_into().unwrap()),
                                    nix::sys::signal::Signal::SIGTERM,
                                );
                                let _ = check_child.wait();
                                return Err(exit_code);
                            } else if sig_pid == check_child.id() {
                                let success =
                                    check_child.wait().is_ok_and(|status| status.success());
                                if success {
                                    return Ok(());
                                } else {
                                    break 'checkLoop;
                                }
                            }
                        }
                    }
                    _ => {
                        let _ = kill(
                            Pid::from_raw(child.id().try_into().unwrap()),
                            nix::sys::signal::Signal::try_from(sig.signal).unwrap(),
                        );
                    }
                }
            }
        }
    }
}

fn propagate_signals(child: &mut Child, signals: &mut SignalsInfo<WithOrigin>) -> i32 {
    loop {
        for sig in signals.wait() {
            match sig.signal {
                SIGCHLD => {
                    if sig.process.and_then(|p| u32::try_from(p.pid).ok()) == Some(child.id()) {
                        return child
                            .wait()
                            .ok()
                            .and_then(|status| status.code())
                            .unwrap_or(1);
                    }
                }
                _ => {
                    let _ = kill(
                        Pid::from_raw(child.id().try_into().unwrap()),
                        nix::sys::signal::Signal::try_from(sig.signal).unwrap(),
                    );
                }
            }
        }
    }
}
