# Fixing Route Table for WSL2

## Step #1 - WSL2 Internet Access

First you need to ensure your WSL2 distributions can access the internet. Before connecting to the VPN your routes for
WSL2 will look something like (using the `Get-NetAdapter` command in powershell):

```
ifIndex DestinationPrefix                              NextHop                                  RouteMetric ifMetric PolicyStore
------- -----------------                              -------                                  ----------- -------- -----------
26      172.31.79.255/32                               0.0.0.0                                          256 5000     ActiveStore
26      172.31.64.1/32                                 0.0.0.0                                          256 5000     ActiveStore
26      172.31.64.0/20                                 0.0.0.0                                          256 5000     ActiveStore
```

But when you connect to the VPN, AnyConnect adds a non-functional route with a lower metric:

```
26      172.31.79.255/32                               0.0.0.0                                          256 5000     ActiveStore
26      172.31.64.1/32                                 0.0.0.0                                          256 5000     ActiveStore
56      172.31.64.0/20                                 10.17.104.1                                        1 1        ActiveStore
26      172.31.64.0/20                                 0.0.0.0                                          256 5000     ActiveStore
```

Unfortunately we cannot remove or modify this route because it will be automatically
[replaced by AnyConnect](https://community.cisco.com/t5/vpn/enforcing-the-split-tunnel-only-access/m-p/4390557/highlight/true#M278089).
However, Windows determines the best route by the lowest sum of interface metric + route metric. What we can do is
increase the AnyConnect interface metric:

```powershell
Get-NetAdapter | Where-Object {$_.InterfaceDescription -Match "Cisco AnyConnect"} | Set-NetIPInterface -InterfaceMetric 6000
```

Now the route table will allow WSL2's NAT connection to the Internet, because 5256 is a lower metric than 6001:

```
26      172.31.79.255/32                               0.0.0.0                                          256 5000     ActiveStore
26      172.31.64.1/32                                 0.0.0.0                                          256 5000     ActiveStore
56      172.31.64.0/20                                 10.17.104.1                                        1 6000     ActiveStore
26      172.31.64.0/20                                 0.0.0.0                                          256 5000     ActiveStore
```

(Unfortunately we still cannot connect from Windows to WSL2 via its IP address because AnyConnect blocks this at the
firewall level using Windows Filtering Platform)

## Step #2 - Automation

The AnyConnect metric will unfortunately be reset every time the VPN is started, so we need to automate this fix
with task scheduler. Save the above [powershell command](./setCiscoVpnMetric.ps1?raw=true) as `setCiscoVpnMetric.ps1`

Open task scheduler and click "Create task":

- Name: "Update AnyConnect Adapter Interface Metric for WSL2"
- Security options: Check "Run with highest privileges"
- Triggers:
  - On an Event, Log: Cisco AnyConnect Secure Mobility Client, Source: acvpnagent, Event ID: 2039
  - On an Event, Log: Cisco AnyConnect Secure Mobility Client, Source: acvpnagent, Event ID: 2041
- Actions: Start a program, Program/script: `powershell.exe`, 
  Add arguments: `-WindowStyle Hidden -NonInteractive -ExecutionPolicy Bypass -File %HOMEPATH%\Documents\setCiscoVpnMetric.ps1`
- Conditions: Uncheck "Start the task only if the computer is on AC power"

## Step #3 - Working Windows DNS

The above fix then leads to a problem for the Windows host, when we look at the routes to the internet the AnyConnect
adapter (56) now has a higher metric than Wi-Fi (17) and Ethernet (13):

```
56      0.0.0.0/0                                      10.17.104.1                                        1 6000     ActiveStore
17      0.0.0.0/0                                      10.2.9.254                                         0 50       ActiveStore
13      0.0.0.0/0                                      10.2.9.254                                         0 25       ActiveStore
```

This will cause Windows to attempt to connect to the now inaccessible DNS servers on Ethernet and Wi-Fi first, causing
up to a 10-second delay in DNS resolution. The solution is to manually update the network interfaces to have a higher
metric than the AnyConnect interface.

Set the Ethernet and Wi-Fi metrics to 6025 and 6050 to ensure they have lower priority than the AnyConnect route (6001)
(Control Panel -> Network and Sharing Center -> Change adapter settings -> Ethernet Properties -> Internet Protocol Version 4 -> Advanced)

```
56      0.0.0.0/0                                      10.17.104.1                                        1 6000     ActiveStore
17      0.0.0.0/0                                      10.2.9.254                                         0 6050     ActiveStore
13      0.0.0.0/0                                      10.2.9.254                                         0 6025     ActiveStore
```
