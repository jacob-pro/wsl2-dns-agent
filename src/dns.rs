use backoff::ExponentialBackoffBuilder;
use itertools::Itertools;
use std::fmt::{Display, Formatter};
use std::mem::transmute;
use std::net::IpAddr;
use std::ptr::{null_mut, slice_from_raw_parts};
use std::time::Duration;
use thiserror::Error;
use win32_utils::net::ToStdSocket;
use win32_utils::str::FromWin32Str;
use windows::Win32::Foundation::{ERROR_BUFFER_OVERFLOW, WIN32_ERROR};
use windows::Win32::NetworkManagement::IpHelper::{
    FreeMibTable, GetAdaptersAddresses, GetIpForwardTable2, GET_ADAPTERS_ADDRESSES_FLAGS,
    IP_ADAPTER_ADDRESSES_LH, MIB_IPFORWARD_ROW2, MIB_IPFORWARD_TABLE2,
};
use windows::Win32::Networking::WinSock::AF_UNSPEC;

#[derive(Debug)]
struct Route {
    interface_index: u32,
    metric: u32,
    destination_prefix_ip: IpAddr,
    destination_prefix_len: u8,
}

impl Route {
    /// If the destination of the route is 0.0.0.0/0 or ::/0
    fn is_internet_route(&self) -> bool {
        self.destination_prefix_ip.is_unspecified() && self.destination_prefix_len == 0
    }
}

/// Returns list of routes to 0.0.0.0/0 and ::/0
fn get_internet_routes() -> Result<Vec<Route>, Error> {
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
                metric: row.Metric,
                destination_prefix_ip: row.DestinationPrefix.Prefix.to_std_socket_addr().ip(),
                destination_prefix_len: row.DestinationPrefix.PrefixLength,
            })
            .filter(Route::is_internet_route)
            .collect::<Vec<_>>();
        FreeMibTable(transmute(ptr));
        Ok(res)
    }
}

#[derive(Debug)]
struct Adapter {
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
            let mut dns_servers = Vec::new();
            let mut next_dns = adapter.FirstDnsServerAddress;
            while !next_dns.is_null() {
                let dns = &*(next_dns);
                dns_servers.push(dns.Address.to_std_socket_addr().ip());
                next_dns = dns.Next;
            }
            let mut dns_suffixes = Vec::new();
            let first_suffix = String::from_pwstr_lossy(adapter.DnsSuffix);
            if first_suffix.len() > 0 {
                dns_suffixes.push(first_suffix);
            }
            let mut next_suffix = adapter.FirstDnsSuffix;
            while !next_suffix.is_null() {
                let suffix = &*(next_suffix);
                dns_suffixes.push(String::from_wchar_lossy(&suffix.String));
                next_suffix = suffix.Next;
            }
            out.push(Adapter {
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

#[derive(Debug)]
struct RouteAndAdapter<'a> {
    route: &'a Route,
    adapter: &'a Adapter,
}

impl RouteAndAdapter<'_> {
    /// "the overall metric that is used to determine the interface preference is the sum of the
    /// route metric and the interface metric"
    fn metric_sum(&self) -> u32 {
        self.route.metric
            + match self.route.destination_prefix_ip {
                IpAddr::V4(_) => self.adapter.ipv4_metric,
                IpAddr::V6(_) => self.adapter.ipv6_metric,
            }
    }
}

impl Display for RouteAndAdapter<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let m_sum = self.metric_sum();
        let s = format!(
            "{}/{}, interface index: {}, metric: {} ({} + {}), dns servers: {:?}, dns suffixes: {:?}",
            self.route.destination_prefix_ip,
            self.route.destination_prefix_len,
            self.route.interface_index,
            m_sum,
            self.route.metric,
            m_sum - self.route.metric,
            self.adapter.dns_servers,
            self.adapter.dns_suffixes
        );
        f.write_str(s.as_str())
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("Unable to find adapter for interface: {interface_index}")]
    RouteInterfaceMismatch { interface_index: u32 },
    #[error("Calls to GetAdaptersAddresses() returned different buffer sizes")]
    GetAdaptersAddressesOverflow,
    #[error("Call to GetIpForwardTable2() failed: {0}")]
    GetIpForwardTable2(#[source] windows::core::Error),
    #[error("Call to GetAdaptersAddresses() failed: {0}")]
    GetAdaptersAddresses(#[source] windows::core::Error),
}

impl Error {
    /// Some errors should be retried
    pub fn into_backoff(self) -> backoff::Error<Self> {
        match &self {
            Error::RouteInterfaceMismatch { .. } => self.into(),
            Error::GetAdaptersAddressesOverflow { .. } => self.into(),
            _ => backoff::Error::Permanent(self),
        }
    }
}

impl From<backoff::Error<Error>> for Error {
    fn from(e: backoff::Error<Error>) -> Self {
        match e {
            backoff::Error::Permanent(e) => e,
            backoff::Error::Transient { err, .. } => err,
        }
    }
}

#[derive(Debug, Default)]
pub struct DnsConfiguration {
    servers: Vec<IpAddr>,
    suffixes: Vec<String>,
}

pub fn get_configuration() -> Result<DnsConfiguration, Error> {
    let op = || {
        {
            let routes = get_internet_routes()?;
            let adapters = get_adapters()?;
            // Match the route interface index with an adapter index
            let mut grouped = routes
                .iter()
                .map(|r| {
                    match r.destination_prefix_ip {
                        IpAddr::V4(_) => adapters
                            .iter()
                            .find(|a| a.ipv4_interface_index.eq(&r.interface_index)),
                        IpAddr::V6(_) => adapters
                            .iter()
                            .find(|a| a.ipv6_interface_index.eq(&r.interface_index)),
                    }
                    .ok_or(Error::RouteInterfaceMismatch {
                        interface_index: r.interface_index,
                    })
                    .map(|a| RouteAndAdapter {
                        route: r,
                        adapter: a,
                    })
                })
                .collect::<Result<Vec<_>, Error>>()?;
            // Sort by the lowest route metrics
            grouped.sort_by_key(|r| r.metric_sum());
            // Get the best routes for IPv4 and IPv6 internets respectively
            let best_v4 = grouped
                .iter()
                .find(|g| g.route.destination_prefix_ip.is_ipv4());
            if let Some(best_v4) = best_v4 {
                log::info!("Best IPv4 Route: {}", best_v4);
            }
            let best_v6 = grouped
                .iter()
                .find(|g| g.route.destination_prefix_ip.is_ipv6());
            if let Some(best_v6) = best_v6 {
                log::info!("Best IPv6 Route: {}", best_v6);
            }
            // Collect the IPv4 and then IPv6 dns configurations
            let mut dns_servers = Vec::new();
            let mut dns_suffixes = Vec::new();
            best_v4.iter().chain(best_v6.iter()).for_each(|g| {
                g.adapter.dns_servers.iter().for_each(|d| {
                    dns_servers.push(d.to_owned());
                });
                g.adapter.dns_suffixes.iter().for_each(|d| {
                    dns_suffixes.push(d.to_owned());
                });
            });
            // Ensure servers and suffixes are unique (preserving order)
            Ok(DnsConfiguration {
                servers: dns_servers.into_iter().unique().collect(),
                suffixes: dns_suffixes.into_iter().unique().collect(),
            })
        }
        .map_err(Error::into_backoff)
    };
    let b = ExponentialBackoffBuilder::new()
        .with_initial_interval(Duration::from_millis(50))
        .with_max_elapsed_time(Some(Duration::from_secs(1)))
        .build();
    backoff::retry(b, op).map_err(|e| e.into())
}
