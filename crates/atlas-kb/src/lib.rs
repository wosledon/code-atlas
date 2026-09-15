//! Chunking and knowledge-base persistence.
//!
//! A page is cut into retrieval units in two possible ways: [`structural_chunks`]
//! (markdown structure, always available offline) or [`semantic_chunk_page`]
//! (the model decides topic boundaries; the stored unit is *summary + verbatim
//! body*). [`store_chunks`] then writes them to the store.

mod persist;
mod semantic;
mod spec;
mod structural;

pub use persist::{link_page_to_module, store_chunks, upsert_module_entities};
pub use semantic::semantic_chunk_page;
pub use spec::{merge_small_chunks, ChunkMode, ChunkSpec};
pub use structural::structural_chunks;
