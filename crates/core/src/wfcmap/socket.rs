use super::{SocketKind, SocketLabel};

/// Returns whether two resolved sockets may occupy facing edges.
///
/// Private sockets and tile strips are intentionally opaque to labels and to
/// each other. Label compatibility is symmetric even for directed labels:
/// a directed label is compatible with its declared partner in either order.
pub fn sockets_match(
    a: &super::ResolvedSocket,
    b: &super::ResolvedSocket,
    labels: &[SocketLabel],
    compat: &[(u32, u32)],
) -> bool {
    use super::ResolvedSocket;

    match (a, b) {
        (ResolvedSocket::Private(left), ResolvedSocket::Private(right)) => left == right,
        (ResolvedSocket::Strip(left), ResolvedSocket::Strip(right)) => left == right,
        (ResolvedSocket::Label(left), ResolvedSocket::Label(right)) => {
            if compat
                .iter()
                .any(|(a, b)| (*a == *left && *b == *right) || (*a == *right && *b == *left))
            {
                return true;
            }
            let left_kind = labels
                .iter()
                .find(|label| label.id == *left)
                .map(|label| &label.kind);
            let right_kind = labels
                .iter()
                .find(|label| label.id == *right)
                .map(|label| &label.kind);
            match (left_kind, right_kind) {
                (Some(SocketKind::Symmetric), Some(SocketKind::Symmetric)) => left == right,
                (Some(SocketKind::Directed { partner }), _) if *partner == *right => true,
                (_, Some(SocketKind::Directed { partner })) if *partner == *left => true,
                _ => false,
            }
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wfcmap::ResolvedSocket;

    fn labels() -> Vec<SocketLabel> {
        vec![
            SocketLabel {
                id: 1,
                name: "symmetric".into(),
                kind: SocketKind::Symmetric,
            },
            SocketLabel {
                id: 2,
                name: "directed".into(),
                kind: SocketKind::Directed { partner: 3 },
            },
            SocketLabel {
                id: 3,
                name: "partner".into(),
                kind: SocketKind::Symmetric,
            },
        ]
    }

    #[test]
    fn separates_socket_families() {
        assert!(sockets_match(
            &ResolvedSocket::Private(4),
            &ResolvedSocket::Private(4),
            &[],
            &[]
        ));
        assert!(!sockets_match(
            &ResolvedSocket::Private(4),
            &ResolvedSocket::Strip(vec![]),
            &[],
            &[]
        ));
        assert!(!sockets_match(
            &ResolvedSocket::Private(4),
            &ResolvedSocket::Label(1),
            &labels(),
            &[]
        ));
    }

    #[test]
    fn labels_obey_kind_and_compatibility() {
        let labels = labels();
        assert!(sockets_match(
            &ResolvedSocket::Label(1),
            &ResolvedSocket::Label(1),
            &labels,
            &[]
        ));
        assert!(sockets_match(
            &ResolvedSocket::Label(2),
            &ResolvedSocket::Label(3),
            &labels,
            &[]
        ));
        assert!(!sockets_match(
            &ResolvedSocket::Label(2),
            &ResolvedSocket::Label(2),
            &labels,
            &[]
        ));
        assert!(sockets_match(
            &ResolvedSocket::Label(1),
            &ResolvedSocket::Label(2),
            &labels,
            &[(1, 2)]
        ));
        assert!(sockets_match(
            &ResolvedSocket::Label(2),
            &ResolvedSocket::Label(1),
            &labels,
            &[(1, 2)]
        ));
    }
}
