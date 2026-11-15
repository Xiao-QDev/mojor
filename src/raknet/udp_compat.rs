//! 跨平台 UDP socket 兼容层
//!
//! 解决 Windows 上 `WSAECONNRESET (10054)` 导致收包循环退出的问题。
//!
//! # 问题
//!
//! Windows 的 UDP 栈在收到 ICMP `port unreachable` 后，会把对应 socket
//! 标记为 reset 状态，下一次 `recv_from` 直接返回 `WSAECONNRESET (10054)`。
//!
//! 触发场景（MCPE 服务端必踩）：
//!
//! 1. 客户端在服务器列表里刷新，发一个 `UnconnectedPing` 然后**立刻关闭 socket**
//! 2. 服务端回 `UnconnectedPong` 时，ICMP 报文回来
//! 3. Windows 标记 socket 为 reset
//! 4. 下一次 `recv_from` 返回 10054
//! 5. `socket.rs` 里 `received?` 把错误往上抛，整个收包循环退出，服务端挂掉
//!
//! Linux/macOS 不受影响：POSIX 下只有 `connect()` 过的 UDP socket 才会因 ICMP
//! 返回 `ECONNREFUSED`，监听用的未 connect socket 完全没事。
//!
//! # 修复
//!
//! 两层防护：
//!
//! - **第一层（根因）**：Windows 上调用 `WSAIoctl(SIO_UDP_CONNRESET, FALSE)`，
//!   让系统忽略这个错误，从源头上不再产生 10054。
//! - **第二层（兜底）**：收包循环里对 `ConnectionReset` / `ConnectionRefused`
//!   做分类处理，只记录不退出，防止其他平台或极端情况下的同类问题。

use std::io;

/// 判断一个 IO 错误是否属于「对端不可达」这类**非致命**错误。
///
/// 这类错误的特点是：只影响本次收发包，socket 本身仍然可用，不应该退出循环。
///
/// - Windows: `WSAECONNRESET (10054)` → [`io::ErrorKind::ConnectionReset`]
/// - Linux: `ECONNREFUSED (111)` → [`io::ErrorKind::ConnectionRefused`]
/// - macOS: `ECONNREFUSED (61)` → [`io::ErrorKind::ConnectionRefused`]
#[inline]
pub fn is_transient_udp_error(err: &io::Error) -> bool {
    matches!(
        err.kind(),
        io::ErrorKind::ConnectionReset | io::ErrorKind::ConnectionRefused
    )
}

/// 在 Windows 上关闭 UDP socket 的 `SIO_UDP_CONNRESET` 行为。
///
/// 泛型参数同时兼容 `std::net::UdpSocket` 和 `tokio::net::UdpSocket`
/// （两者都实现了 `AsRawSocket`）。
///
/// 非 Windows 平台是空操作，编译后零开销。
///
/// 必须在 `bind()` 之后、开始收发之前调用。
#[cfg(windows)]
pub fn fix_udp_socket<S>(socket: &S) -> io::Result<()>
where
    S: std::os::windows::io::AsRawSocket,
{
    windows_impl::disable_udp_connreset(socket.as_raw_socket())
}

/// 非 Windows 平台：空操作。
#[cfg(not(windows))]
#[inline]
#[allow(unused_variables)]
pub fn fix_udp_socket<S>(_socket: &S) -> io::Result<()> {
    Ok(())
}

#[cfg(windows)]
mod windows_impl {
    use std::io;
    use std::os::windows::io::RawSocket;

    /// `SIO_UDP_CONNRESET`，定义见 `mswsock.h`。
    const SIO_UDP_CONNRESET: u32 = 0x9800_0012;

    #[link(name = "ws2_32")]
    extern "system" {
        fn WSAIoctl(
            socket: usize,
            control_code: u32,
            in_buffer: *mut std::ffi::c_void,
            in_buffer_size: u32,
            out_buffer: *mut std::ffi::c_void,
            out_buffer_size: u32,
            bytes_returned: *mut u32,
            overlapped: *mut std::ffi::c_void,
            completion_routine: *mut std::ffi::c_void,
        ) -> i32;
    }

    pub(super) fn disable_udp_connreset(raw: RawSocket) -> io::Result<()> {
        // 传 FALSE (0) 表示「不要因为 ICMP 而 reset」
        let mut enable: u32 = 0;
        let mut bytes_returned: u32 = 0;

        // SAFETY: raw 是调用方提供的有效 socket 句柄；enable 是 u32，
        // 大小与 WSAIoctl 声明的 in_buffer_size 一致。
        let ret = unsafe {
            WSAIoctl(
                raw as usize,
                SIO_UDP_CONNRESET,
                &mut enable as *mut u32 as *mut std::ffi::c_void,
                std::mem::size_of::<u32>() as u32,
                std::ptr::null_mut(),
                0,
                &mut bytes_returned,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };

        if ret == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_reset_is_transient() {
        let e = io::Error::from(io::ErrorKind::ConnectionReset);
        assert!(is_transient_udp_error(&e));
    }

    #[test]
    fn connection_refused_is_transient() {
        let e = io::Error::from(io::ErrorKind::ConnectionRefused);
        assert!(is_transient_udp_error(&e));
    }

    #[test]
    fn other_errors_are_fatal() {
        let e = io::Error::from(io::ErrorKind::PermissionDenied);
        assert!(!is_transient_udp_error(&e));
    }

    #[test]
    fn fix_udp_socket_succeeds_on_std_socket() {
        let sock = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        assert!(fix_udp_socket(&sock).is_ok());
    }
}
