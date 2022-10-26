use crate::APP_NAME;
use itertools::Itertools;
use std::mem::transmute;
use std::net::IpAddr;
use std::ptr::{null_mut, slice_from_raw_parts};
use std::slice;
use thiserror::Error;
use win32_utils::net::ToStdSocket;
use win32_utils::str::FromWin32Str;
use windows::Win32::Foundation::{ERROR_BUFFER_OVERFLOW, WIN32_ERROR};
use windows::Win32::NetworkManagement::IpHelper::{
    FreeMibTable, GetAdaptersAddresses, GetIpForwardTable2, GET_ADAPTERS_ADDRESSES_FLAGS,
    IP_ADAPTER_ADDRESSES_LH, MAXLEN_IFDESCR, MIB_IPFORWARD_ROW2, MIB_IPFORWARD_TABLE2,
};
use windows::Win32::Networking::WinSock::AF_UNSPEC;

#[derive(Debug)]
struct Route {
    interface_index: u32,
    destination_prefix_ip: IpAddr,
    destination_prefix_len: u8,
}

impl Route {
    /// If the destination of the route is 0.0.0.0/0 or ::/0
    fn is_internet_route(&self) -> bool {
        self.destination_prefix_ip.is_unspecified() && self.destination_prefix_len == 0
    }
}

/// Returns a list of IPv4 and IPv6 routes
fn get_routes() -> Result<Vec<Route>, Error> {
    unsafe {
        let mut ptr = null_mut::<MIB_IPFORWARD_TABLE2>();
        GetIpForwardTable2(AF_UNSPEC.0 as u16, &mut ptr).map_err(Error::GetIpForwardTable2)?;
        let deref = &*ptr;
        let table = slice_from_raw_parts(
            &deref.Table as *const MIB_IPFORWARD_ROW2,
            deref.NumEntries as usize,
        );
        let table = &*table;
        let res = (0..deref.NumEntries)
            .map(|idx| &table[idx as usize])
            .map(|row| Route {
                interface_index: row.InterfaceIndex,
                destination_prefix_ip: row.DestinationPrefix.Prefix.to_std_socket_addr().ip(),
                destination_prefix_len: row.DestinationPrefix.PrefixLength,
            })
            .collect::<Vec<_>>();
        FreeMibTable(transmute(ptr));
        Ok(res)
    }
}

#[derive(Debug)]
struct Adapter {
    description: String,
    ipv4_metric: u32,
    ipv6_metric: u32,
    ipv4_interface_index: u32,
    ipv6_interface_index: u32,
    dns_servers: Vec<IpAddr>,
    dns_suffixes: Vec<String>,
}

/// Returns a list of the system network adapters
fn get_adapters() -> Result<Vec<Adapter>, Error> {
    unsafe {
        let mut length = 0;
        let e = WIN32_ERROR(GetAdaptersAddresses(
            AF_UNSPEC,
            GET_ADAPTERS_ADDRESSES_FLAGS(0),
            null_mut(),
            null_mut(),
            &mut length,
        ));
        if e != ERROR_BUFFER_OVERFLOW {
            return Err(Error::GetAdaptersAddresses(windows::core::Error::from(e)));
        }
        let mut buffer = Vec::<u8>::with_capacity(length as usize);
        let e = WIN32_ERROR(GetAdaptersAddresses(
            AF_UNSPEC,
            GET_ADAPTERS_ADDRESSES_FLAGS(0),
            null_mut(),
            transmute(buffer.as_mut_ptr()),
            &mut length,
        ));
        if e.is_err() {
            if e == ERROR_BUFFER_OVERFLOW {
                return Err(Error::GetAdaptersAddressesOverflow);
            }
            return Err(Error::GetAdaptersAddresses(windows::core::Error::from(e)));
        }
        let mut next = buffer.as_ptr() as *const IP_ADAPTER_ADDRESSES_LH;
        let mut out = Vec::new();
        while !next.is_null() {
            let adapter = &*(next);
            // Description
            let description_buffer =
                slice::from_raw_parts(adapter.Description.0, MAXLEN_IFDESCR.try_into().unwrap())
                    .split(|&c| c == 0)
                    .next()
                    .unwrap();
            let description = String::from_utf16_lossy(&description_buffer);
            // DNS Servers
            let mut dns_servers = Vec::new();
            let mut next_dns = adapter.FirstDnsServerAddress;
            while !next_dns.is_null() {
                let dns = &*(next_dns);
                dns_servers.push(dns.Address.to_std_socket_addr().ip());
                next_dns = dns.Next;
            }
            // Suffixes
            let mut dns_suffixes = Vec::new();
            let first_suffix = String::from_pwstr_lossy(adapter.DnsSuffix);
            if !first_suffix.is_empty() {
                dns_suffixes.push(first_suffix);
            }
            let mut next_suffix = adapter.FirstDnsSuffix;
            while !next_suffix.is_null() {
                let suffix = &*(next_suffix);
                dns_suffixes.push(String::from_wchar_lossy(&suffix.String));
                next_suffix = suffix.Next;
            }

            out.push(Adapter {
                description: description,
                ipv4_metric: adapter.Ipv4Metric,
                ipv6_metric: adapter.Ipv6Metric,
                ipv4_interface_index: adapter.Anonymous1.Anonymous.IfIndex,
                ipv6_interface_index: adapter.Ipv6IfIndex,
                dns_servers,
                dns_suffixes,
            });
            next = adapter.Next;
        }
        Ok(out)
    }
}

impl Adapter {
    // For the purposes of DNS, the interface metric is whichever one is lowest
    fn interface_metric(&self) -> u32 {
        std::cmp::min(self.ipv4_metric, self.ipv6_metric)
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("Calls to GetAdaptersAddresses() returned different buffer sizes")]
    GetAdaptersAddressesOverflow,
    #[error("Call to GetIpForwardTable2() failed: {0}")]
    GetIpForwardTable2(#[source] windows::core::Error),
    #[error("Call to GetAdaptersAddresses() failed: {0}")]
    GetAdaptersAddresses(#[source] windows::core::Error),
}

#[derive(Debug, Default)]
pub struct DnsConfiguration {
    servers: Vec<IpAddr>,
    suffixes: Vec<String>,
}

pub fn get_configuration() -> Result<DnsConfiguration, Error> {
    let adapters = get_adapters()?;

    let internet_adapters: Vec<Adapter>;

    let vpn_enabled = adapters
        .iter()
        .any(|adapter| adapter.description.contains("Cisco AnyConnect"));

    if vpn_enabled {
        // Get the adapters created by Cisco Anyconnect
        internet_adapters = adapters
            .into_iter()
            .filter(|adapter| adapter.description.contains("Cisco AnyConnect"))
            .sorted_by_key(Adapter::interface_metric)
            .collect::<Vec<_>>();
    } else {
        // List of routes to the internet
        let internet_routes = get_routes()?
            .into_iter()
            .filter(Route::is_internet_route)
            .collect::<Vec<_>>();

        // DNS priority is determined by interface metric
        // However we also want to exclude various system adapters such as WSL
        // so we will filter out any adapters that don't have a route to the internet
        internet_adapters = adapters
            .into_iter()
            .filter(|adapter| {
                internet_routes
                    .iter()
                    .any(|route| match route.destination_prefix_ip {
                        IpAddr::V4(_) => route.interface_index == adapter.ipv4_interface_index,
                        IpAddr::V6(_) => route.interface_index == adapter.ipv6_interface_index,
                    })
            })
            .sorted_by_key(Adapter::interface_metric)
            .collect::<Vec<_>>();
    }

    let servers = internet_adapters
        .iter()
        .flat_map(|adapter| adapter.dns_servers.clone())
        .unique()
        .collect::<Vec<_>>();
    let suffixes = internet_adapters
        .iter()
        .flat_map(|adapter| adapter.dns_suffixes.clone())
        .unique()
        .collect::<Vec<_>>();

    Ok(DnsConfiguration { servers, suffixes })
}

impl DnsConfiguration {
    pub fn generate_resolv(&self) -> String {
        let date = chrono::Local::now();
        let date = format!("{}", date.format("%Y-%m-%d %H:%M:%S"));
        let mut lines = vec![format!("# Generated by {APP_NAME} at {date}")];
        // WSL2 doesn't currently support IPv6 - but might do in future?
        // https://github.com/microsoft/WSL/issues/4518
        // resolv.conf typically only allows up to 3 nameservers
        self.servers
            .iter()
            .filter(|server| server.is_ipv4())
            .take(3)
            .for_each(|server| lines.push(format!("nameserver {}", server)));

        if !self.suffixes.is_empty() {
            lines.push(format!("search {}", self.suffixes.join(" ")));
        }

        lines.push(String::new());
        lines.join("\n")
    }
}
