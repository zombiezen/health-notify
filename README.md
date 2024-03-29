# health-notify

`health-notify` is a small helper program for systemd services
that runs a health check program periodically on startup until the check program succeeds
and then notifies systemd using [`sd_notify`][].
This is useful for deploying programs that do not support [socket activation][]
and having them behave with other services
that depend on the program already listening for a socket.

[`sd_notify`]: https://0pointer.de/blog/projects/socket-activation.html
[socket activation]: https://0pointer.de/blog/projects/socket-activation.html

## Usage

HTTP-based health check:

```ini
[Unit]
Wants=local-fs.target network-online.target
After=local-fs.target network-online.target

[Service]
# Wrap your program arguments in health-notify.
# Everything before the first semicolon are the main program arguments.
# Everything after the first semicolon specify the health check program,
# which should exit with status code 0 on success.
ExecStart=/usr/local/bin/health-notify /usr/local/bin/my-server --port=8080 \; /usr/bin/curl -fsSL http://localhost:8080
# Use Type=notify to have systemd wait until a notification
# to mark the unit as started.
Type=notify

Restart=always
```

TCP-based health check:

```ini
[Unit]
Wants=local-fs.target network-online.target
After=local-fs.target network-online.target

[Service]
ExecStart=/usr/local/bin/health-notify /usr/local/bin/my-server --port=8080 \; /usr/bin/nc -z localhost 8080
Type=notify
Restart=always
```

## License

[Apache 2.0](LICENSE)
