# WSL2 DNS Agent for Cisco AnyConnect Users

[![Build status](https://github.com/jacob-pro/wsl2-dns-agent/actions/workflows/rust.yml/badge.svg)](https://github.com/jacob-pro/wsl2-dns-agent/actions)

An agent that automatically patches your WSL2 DNS configuration when using Cisco AnyConnect (or similar VPNs that block
split-tunneling).

Thanks to @pyther for the [inspiration for this tool](https://gist.github.com/pyther/b7c03579a5ea55fe431561b502ec1ba8).

## How it works

1. The agent detects when you connect/disconnect from a VPN.
2. The agent finds the highest priority DNS servers being used by Windows.
3. The agent detects your WSL2 distributions, for each distribution it ensures that `generateResolvConf` is disabled, 
   and then writes the DNS servers to `/etc/resolv.conf`.

## Usage

**Ensure you have first fixed the route table for WSL2, and not broken the Windows DNS server priority in the process**.
See the [guide](./docs/ROUTING.md) for how to do this.

Simply download `wsl2-dns-agent.exe` from the [releases page](https://github.com/jacob-pro/wsl2-dns-agent/releases/latest)

Save it to your startup folder (`%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup`).

Launch the application.

Note: You must have the `chattr` command installed within your WSL2 distribution.

## Diagnostics

You can view the application log by clicking on the tray icon and "View Log".

Note that this tool *should* apply DNS servers based on their priority in Windows.

For example, from Windows Command Prompt try running:

```cmd
C:\Users\jdhalsey>nslookup.exe google.com
Server:  OpenWrt.lan
Address:  10.2.9.254

Non-authoritative answer: ...
```

Therefore `10.2.9.254` will be the first server written to `/etc/resolv.conf`. If the server is not what you expected
then please look at [the DNS guide](./docs/ROUTING.md#step-3---working-windows-dns)

## Advanced options

For advanced use cases you can edit the config file in `%APPDATA%\WSL2 DNS Agent\config.toml`

Example config:

```
show_notifications = false

# Default options for distributions
[defaults]
apply_dns = true
patch_wsl_conf = true
# If the distribution was previously Stopped, then shutdown once the DNS update is complete
# Note: This option is usually not needed on Windows 11 (because vmIdleTimeout will do it for you)
shutdown = false

# Set options for a specific distribution
[distributions.Ubuntu]
apply_dns = false
```

Note: the default configuration will ignore Docker Desktop, since the changes are unnecessary.
