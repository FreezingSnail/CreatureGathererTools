use std::collections::HashSet;

use creaturevm::wfcmap::{
    CaptureOpts, EdgeSocket, ResolvedSocket, Side, SocketKind, SocketLabel, TileRect, WfcPiece,
    capture_piece, decode, encode, slice_piece, sockets_match,
};

fn source() -> Vec<Vec<u16>> {
    (0..256)
        .map(|y| (0..256).map(|x| (y * 256 + x) as u16).collect())
        .collect()
}

fn opts() -> CaptureOpts {
    CaptureOpts {
        name: "capture".into(),
        weight: 1.5,
        limit: Some(4),
    }
}

fn captured_piece(id: u32) -> WfcPiece {
    capture_piece(
        &source(),
        TileRect {
            x: 3,
            y: 5,
            w: 7,
            h: 3,
        },
        2,
        2,
        id,
        &opts(),
    )
    .expect("capture")
}

#[test]
fn capture_snaps_and_extracts_row_major_tiles() {
    let piece = captured_piece(11);
    assert_eq!((piece.tw, piece.th), (8, 4));
    assert_eq!(piece.name, "capture");
    assert_eq!(piece.weight, 1.5);
    assert_eq!(piece.limit, Some(4));
    let tiles = decode(&piece.tiles, 32).expect("captured RLE");
    let expected: Vec<u16> = (4..8)
        .flat_map(|y| (2..10).map(move |x| (y * 256 + x) as u16))
        .collect();
    assert_eq!(tiles, expected);
}

#[test]
fn capture_rejects_empty_and_out_of_bounds_regions() {
    let map = source();
    assert!(
        capture_piece(
            &map,
            TileRect {
                x: 0,
                y: 0,
                w: 0,
                h: 1
            },
            2,
            2,
            1,
            &opts()
        )
        .is_err()
    );
    assert!(
        capture_piece(
            &map,
            TileRect {
                x: 255,
                y: 0,
                w: 2,
                h: 1
            },
            2,
            2,
            1,
            &opts()
        )
        .is_err()
    );
    assert!(
        capture_piece(
            &map,
            TileRect {
                x: 0,
                y: 255,
                w: 1,
                h: 2
            },
            2,
            2,
            1,
            &opts()
        )
        .is_err()
    );
}

#[test]
fn slices_preserve_blocks_and_anchor_only_once() {
    let piece = captured_piece(12);
    let slices = slice_piece(&piece, 2, 2).expect("slice");
    assert_eq!(slices.len(), 8);
    assert_eq!(slices.iter().filter(|slice| slice.is_anchor).count(), 1);
    assert_eq!((slices[0].ox, slices[0].oy), (0, 0));
    let full = decode(&piece.tiles, (piece.tw * piece.th) as usize).unwrap();
    for slice in slices {
        let col = slice.ox / 2;
        let row = slice.oy / 2;
        let expected: Vec<u16> = (0..2)
            .flat_map(|dy| {
                let start = ((row * 2 + dy) * piece.tw + col * 2) as usize;
                full[start..start + 2].iter().copied()
            })
            .collect();
        assert_eq!(slice.tiles, expected);
    }
}

#[test]
fn internal_private_edges_are_shared_and_piece_scoped() {
    let one = slice_piece(&captured_piece(20), 2, 2).unwrap();
    let two = slice_piece(&captured_piece(21), 2, 2).unwrap();
    let mut one_private = HashSet::new();
    let mut two_private = HashSet::new();
    for slice in &one {
        for edge in &slice.edges {
            if let ResolvedSocket::Private(id) = edge {
                one_private.insert(*id);
            }
        }
    }
    for slice in &two {
        for edge in &slice.edges {
            if let ResolvedSocket::Private(id) = edge {
                two_private.insert(*id);
            }
        }
    }
    assert!(!one_private.is_empty());
    assert!(one_private.is_disjoint(&two_private));
    assert_eq!(one[0].edges[1], one[1].edges[3]);
    assert_eq!(one[0].edges[2], one[4].edges[0]);
}

#[test]
fn boundary_strips_use_shared_cell_order() {
    let mut piece = captured_piece(30);
    piece.tw = 4;
    piece.th = 4;
    piece.tiles = encode(&(0..16).collect::<Vec<_>>());
    let slices = slice_piece(&piece, 2, 2).unwrap();
    assert_eq!(slices[0].edges[0], ResolvedSocket::Strip(vec![0, 1]));
    assert_eq!(slices[0].edges[3], ResolvedSocket::Strip(vec![0, 4]));
    assert_eq!(slices[1].edges[0], ResolvedSocket::Strip(vec![2, 3]));
    assert_eq!(slices[1].edges[1], ResolvedSocket::Strip(vec![3, 7]));
    assert_eq!(slices[2].edges[2], ResolvedSocket::Strip(vec![12, 13]));
    assert_eq!(slices[2].edges[3], ResolvedSocket::Strip(vec![8, 12]));
}

#[test]
fn boundary_override_replaces_one_edge_only() {
    let mut piece = captured_piece(40);
    piece.tw = 4;
    piece.th = 4;
    piece.tiles = encode(&(0..16).collect::<Vec<_>>());
    piece.edge_overrides.push(EdgeSocket {
        side: Side::N,
        index: 1,
        label: 99,
    });
    let slices = slice_piece(&piece, 2, 2).unwrap();
    assert_eq!(slices[1].edges[0], ResolvedSocket::Label(99));
    assert_eq!(slices[0].edges[0], ResolvedSocket::Strip(vec![0, 1]));
    assert_eq!(slices[1].edges[1], ResolvedSocket::Strip(vec![3, 7]));
}

#[test]
fn invalid_override_is_not_silently_ignored() {
    let mut piece = captured_piece(41);
    piece.edge_overrides.push(EdgeSocket {
        side: Side::N,
        index: 99,
        label: 1,
    });
    assert!(slice_piece(&piece, 2, 2).is_err());
}

#[test]
fn socket_matching_observes_private_strip_label_rules() {
    assert!(sockets_match(
        &ResolvedSocket::Private(1),
        &ResolvedSocket::Private(1),
        &[],
        &[]
    ));
    assert!(!sockets_match(
        &ResolvedSocket::Private(1),
        &ResolvedSocket::Private(2),
        &[],
        &[]
    ));
    assert!(sockets_match(
        &ResolvedSocket::Strip(vec![1, 2]),
        &ResolvedSocket::Strip(vec![1, 2]),
        &[],
        &[]
    ));
    assert!(!sockets_match(
        &ResolvedSocket::Strip(vec![1, 2]),
        &ResolvedSocket::Strip(vec![2, 1]),
        &[],
        &[]
    ));
    assert!(!sockets_match(
        &ResolvedSocket::Private(1),
        &ResolvedSocket::Label(1),
        &[],
        &[]
    ));

    let labels = vec![
        SocketLabel {
            id: 1,
            name: "road".into(),
            kind: SocketKind::Symmetric,
        },
        SocketLabel {
            id: 2,
            name: "in".into(),
            kind: SocketKind::Directed { partner: 3 },
        },
        SocketLabel {
            id: 3,
            name: "out".into(),
            kind: SocketKind::Symmetric,
        },
        SocketLabel {
            id: 4,
            name: "other".into(),
            kind: SocketKind::Symmetric,
        },
    ];
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
        &ResolvedSocket::Label(4),
        &labels,
        &[]
    ));
    assert!(sockets_match(
        &ResolvedSocket::Label(1),
        &ResolvedSocket::Label(4),
        &labels,
        &[(1, 4)]
    ));
    assert!(sockets_match(
        &ResolvedSocket::Label(4),
        &ResolvedSocket::Label(1),
        &labels,
        &[(1, 4)]
    ));
}
