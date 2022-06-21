# Windows 10 DNS Priority

The way DNS priority works 
[changed in Windows 10](https://web.archive.org/web/20190106092511/https://blogs.technet.microsoft.com/networking/2015/08/14/adjusting-the-network-protocol-bindings-in-windows-10/)
to use interface metric instead of binding order. However, it isn't entirely clear from the docs precisely how this
priority works under certain scenarios, so I have done some experiments:

## Testing Interface Metrics

Ethernet 1 (index 9), DNS server: 10.1.1.254 \
Ethernet 2 (index 24), DNS server: 10.2.2.254

```powershell
Set-NetIPInterface -InterfaceAlias "Ethernet 1" -InterfaceMetric 50
Set-NetIPInterface -InterfaceAlias "Ethernet 2" -InterfaceMetric 100
```

Ethernet 1 wins:

```powershell
nslookup google.com
Address:  10.1.1.254
```

Let's flip the metrics:

```powershell
Set-NetIPInterface -InterfaceAlias "Ethernet 1" -InterfaceMetric 100
Set-NetIPInterface -InterfaceAlias "Ethernet 2" -InterfaceMetric 50
```

Now Ethernet 2 wins, this is expected:

```powershell
nslookup google.com
Address:  10.2.2.254
```

## Does Route Priority Matter?

Ethernet 1 (index 9), DNS server: 10.1.1.254 \
Ethernet 2 (index 24), DNS server: 10.2.2.254

```powershell
Set-NetIPInterface -InterfaceAlias "Ethernet 1" -InterfaceMetric 50
Set-NetIPInterface -InterfaceAlias "Ethernet 2" -InterfaceMetric 100
Set-NetRoute -DestinationPrefix 0.0.0.0/0 -InterfaceAlias "Ethernet 1" -RouteMetric 256
Set-NetRoute -DestinationPrefix 0.0.0.0/0 -InterfaceAlias "Ethernet 2" -RouteMetric 256
```

Initially Ethernet 1 wins for DNS:

```powershell
nslookup google.com
Address:  10.1.1.254
```

And also routing (because 256 + 50 is less than 256 + 100):

```powershell
Find-NetRoute -RemoteIPAddress "8.8.8.8" | Select-Object InterfaceAlias
Ethernet 1
```

But what if we change the route priority:

```powershell
Set-NetRoute -DestinationPrefix 0.0.0.0/0 -InterfaceAlias "Ethernet 1" -RouteMetric 9999
Set-NetRoute -DestinationPrefix 0.0.0.0/0 -InterfaceAlias "Ethernet 2" -RouteMetric 5
```

Such that Ethernet 2 is the best route (5 + 100 is less than 9999 + 50)

```powershell
Find-NetRoute -RemoteIPAddress "8.8.8.8" | Select-Object InterfaceAlias
Ethernet 2
```

The best DNS server is still on Ethernet 1:

```powershell
nslookup google.com
Address:  10.1.1.254
```

So it would appear the route metrics are irrelevant.

## How are IPv4 vs IPv6 interface metrics treated

Note: Neither Ethernet adapters have an IPv6 address/route or DNS server.

```powershell
# Ethernet 1 has a better IPv4 metric (50)
Set-NetIPInterface -InterfaceAlias "Ethernet 1" -AddressFamily IPv4 -InterfaceMetric 50
Set-NetIPInterface -InterfaceAlias "Ethernet 2" -AddressFamily IPv4 -InterfaceMetric 100

# However Ethernet 2 has the best metric overall (25):
Set-NetIPInterface -InterfaceAlias "Ethernet 1" -AddressFamily IPv6 -InterfaceMetric 50
Set-NetIPInterface -InterfaceAlias "Ethernet 2" -AddressFamily IPv6 -InterfaceMetric 25
```

Ethernet 2 now has the highest priority DNS (even though IPv6 isn't in use):

```powershell
nslookup google.com
Address:  10.2.2.254
```

## DNS servers on the same interface

In this scenario only one Ethernet adapter is enabled:

IPv4 DNS server: 10.1.1.254 \
IPv6 DNS server: ::1

```powershell
Set-NetIPInterface -InterfaceAlias "Ethernet 1" -InterfaceMetric 50
```

When both IPv4 and IPv6 metrics are equal the IPv6 DNS server is used first:

```powershell
nslookup google.com
Address:  ::1
```

What about if the IPv4 metric is lower:

```powershell
Set-NetIPInterface -InterfaceAlias "Ethernet 1" -AddressFamily IPv4 -InterfaceMetric 50
Set-NetIPInterface -InterfaceAlias "Ethernet 1" -AddressFamily IPv6 -InterfaceMetric 100
```

It doesn't matter, the IPv6 DNS server is still chosen first:

```powershell
nslookup google.com
Address:  ::1
```
