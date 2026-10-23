pub mod actions;
pub mod assembler;
pub mod cache;
pub mod ctx;
pub mod data;
pub mod evaluator;
pub mod matcher;
pub mod middleware;
pub mod meta;
pub mod resolver;
pub mod response;
pub mod natives;
pub mod paths;
pub mod reads;
pub mod segments;
pub mod services;
pub mod islands;
pub mod store;
pub mod stream;
pub mod wire;

pub use actions::{ActionError, ActionHandler, ActionRegistry, FailureKind};
pub use assembler::{
  AssembleError, Assembly, Evaluators, Origin, PENDING_PROP, PendingResolution, Resolved, Runtime, RuntimeBuilder, assemble, assemble_under,
};
pub use cache::{CacheEntry, FibreCache, LoadCache, MemoryCache, MemoryLoadCache, NoCache, NoLoadCache, NodeCache, WarmLoads, WarmRenders};
pub use ctx::{Address, CsrfHandle, FAILURE_KEY, Identity, Locale, RedirectHandle, RequestCtx, SessionCell, unix_now, parse_query};
pub use data::{DataSource, DataSources, LoadError, LoadKeyer, NoLoadKey};
pub use evaluator::{slot_regions, Chunk, EvalError, Evaluator, NodeChunks, NullEvaluator};
pub use islands::{IslandEvent, IslandHandler, IslandRegistry, island_data};
pub use matcher::{EntryId, HandlerMatch, HandlerMatcher, Matcher, MatchitMatcher, RouteMatch};
pub use meta::{is_data_block, DocumentTheme, Head, HeadEl, HeadRows, Meta, Metadata, PATH_MARK};
pub use resolver::{Resolver, TableResolver};
pub use response::{HttpResponse, REDIRECT_STATUSES, RESPONSE_TAG};
pub use middleware::{drive_middleware, NextHandle, Passed, Target, REQUEST_TAG};
pub use segments::{DefaultKeyer, SegmentInfo, SegmentKeyer};
pub use natives::{FromNativeValue, IntoNativeValue, Native, NativeHandle, Natives, native_arg};
pub use futures_util::future::BoxFuture;

/// What a `#[native]` or `#[service]` type needs: each call's future owns a
/// copy of it, so it is cloned per call. Shared state goes behind an `Arc`.
#[diagnostic::on_unimplemented(
  message = "`{Self}` is a `#[native]` or `#[service]` type and needs `Clone`",
  label = "each call clones it",
  note = "derive `Clone` on `{Self}` and keep shared state behind an `Arc`"
)]
pub trait CallState: Clone + Send + Sync + 'static {}

impl<T: Clone + Send + Sync + 'static> CallState for T {}
pub use services::{ServiceCaller, ServiceError, ServiceHandle};
pub use paths::Paths;
pub use reads::{subtree_shape, Reads, Static, SubtreeReads, DOCUMENT_PROP, PATH_PROP};
pub use store::{Contribution, Seeds, contribution_order, merge_contributions, pending_keys};
pub use stream::{FILL_SCRIPT, fragment_html, html_stream, contributions_to_json, meta_to_json, segments_from_json, segments_to_json, wire_stream};
