pub mod bake;
pub mod capture;
pub mod compile;
pub mod generate;
pub mod rle;
pub mod schema;
pub mod slice;
pub mod socket;

pub use bake::{Hunk, bake};
pub use capture::{CaptureError, CaptureOpts, TileRect, capture_piece};
pub use compile::{CompiledRecipe, SymbolInfo, compile_recipe, compile_recipe_for_size};
pub use generate::{GenerateError, generate};
pub use rle::{RleError, decode, encode};
pub use schema::{
    Anchor, EdgeSocket, Side, SocketKind, SocketLabel, Stamp, StampOrigin, ValidationIssue,
    WfcPiece, WfcProject, WfcRecipe,
};
pub use slice::{ResolvedSocket, Slice, SliceError, slice_piece, validate_edge_overrides};
pub use socket::sockets_match;
