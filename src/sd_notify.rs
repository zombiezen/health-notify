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

use std::env;
use std::io;
use std::os::unix::net::UnixDatagram;
use std::path::PathBuf;

use crate::lazy_fail_init::LazyFailInit;

pub(crate) const ENV_VAR: &str = "NOTIFY_SOCKET";

#[derive(Debug)]
pub(crate) struct SystemdNotify {
    socket_path: PathBuf,
    socket: LazyFailInit<UnixDatagram>,
}

impl SystemdNotify {
    pub(crate) fn from_env() -> Option<Self> {
        let socket_path = env::var_os(ENV_VAR).unwrap_or_default();
        env::remove_var(ENV_VAR);
        if socket_path.is_empty() {
            None
        } else {
            Some(SystemdNotify {
                socket_path: socket_path.into(),
                socket: LazyFailInit::new(),
            })
        }
    }

    pub(crate) unsafe fn take_from_env() -> Option<Self> {
        let sd_notify = Self::from_env();
        env::remove_var(ENV_VAR);
        sd_notify
    }

    pub(crate) fn notify(&self, buf: impl AsRef<[u8]>) -> io::Result<()> {
        let socket = self
            .socket
            .get_or_create(|| UnixDatagram::bind(&self.socket_path))?;
        socket.send(buf.as_ref()).map(|_| ())
    }
}
