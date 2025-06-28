//! Helper that owns the debounce/cancellation logic for `@` file searches.
//!
//! `ChatComposer` publishes *every* change of the `@token` as
//! `AppEvent::StartFileSearch(query)`.
//! This struct receives those events and decides when to actually spawn the
//! expensive search (handled in the main `App` thread). It guarantees:
//!
//! 1. First query is forwarded immediately.
//! 2. While a search is in-flight a debounce window (200 ms) is enforced.
//! 3. If the user keeps extending the current query (old-query is prefix of
//!    new-query) we keep the running search; otherwise we cancel it.
//! 4. At most one debounce timer thread runs at a time.

use codex_file_search as file_search;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;

// Debouncing is handled via `pending_query` in `SearchState`.

#[allow(clippy::unwrap_used)]
const MAX_FILE_SEARCH_RESULTS: NonZeroUsize = NonZeroUsize::new(8).unwrap();

#[allow(clippy::unwrap_used)]
const NUM_FILE_SEARCH_THREADS: NonZeroUsize = NonZeroUsize::new(2).unwrap();

/// State machine for file-search orchestration.
pub(crate) struct FileSearchManager {
    /// Unified state guarded by one mutex.
    state: Arc<Mutex<SearchState>>,

    search_dir: PathBuf,
    app_tx: AppEventSender,
}

struct SearchState {
    in_flight: Option<InFlightSearch>,
    pending_query: Option<String>,
}

struct InFlightSearch {
    query: String,
    cancellation_token: Arc<AtomicBool>,
}

impl FileSearchManager {
    pub fn new(search_dir: PathBuf, tx: AppEventSender) -> Self {
        Self {
            state: Arc::new(Mutex::new(SearchState {
                in_flight: None,
                pending_query: None,
            })),
            search_dir,
            app_tx: tx,
        }
    }

    /// Call whenever the user edits the `@` token.
    pub fn on_user_query(&mut self, query: String) {
        // This will hold information about a search we need to kick off once
        // we drop the mutex.
        let (query, token): (String, Arc<AtomicBool>) = {
            #[allow(clippy::unwrap_used)]
            let mut st = self.state.lock().unwrap();
            match st.in_flight.as_ref() {
                Some(in_flight) => {
                    if query.starts_with(&in_flight.query) {
                        // Still compatible – just queue.
                        st.pending_query = Some(query);
                        return;
                    }

                    // Cancel current search and replace with new.
                    in_flight.cancellation_token.store(true, Ordering::Relaxed);

                    let token = Arc::new(AtomicBool::new(false));
                    st.in_flight = Some(InFlightSearch {
                        query: query.clone(),
                        cancellation_token: token.clone(),
                    });
                    st.pending_query = None;
                    (query.clone(), token)
                }
                None => {
                    let token = Arc::new(AtomicBool::new(false));
                    st.in_flight = Some(InFlightSearch {
                        query: query.clone(),
                        cancellation_token: token.clone(),
                    });
                    st.pending_query = None;
                    (query.clone(), token)
                }
            }
        };

        self.fire_search(query, token);
    }

    /// Caller is responsible for ensuring self.in_flight is not None
    /// when calling this method.
    fn fire_search(&self, query: String, cancellation_token: Arc<AtomicBool>) {
        Self::spawn_file_search(
            query.clone(),
            self.search_dir.clone(),
            self.app_tx.clone(),
            cancellation_token.clone(),
            self.state.clone(),
        );
    }

    fn spawn_file_search(
        query: String,
        search_dir: PathBuf,
        tx: AppEventSender,
        cancellation_token: Arc<AtomicBool>,
        state: Arc<Mutex<SearchState>>,
    ) {
        std::thread::spawn(move || {
            let matches = file_search::run(
                &query,
                MAX_FILE_SEARCH_RESULTS,
                &search_dir,
                Vec::new(),
                NUM_FILE_SEARCH_THREADS,
                cancellation_token.clone(),
            )
            .map(|res| {
                res.matches
                    .into_iter()
                    .map(|(_, p)| p)
                    .collect::<Vec<String>>()
            })
            .unwrap_or_default();

            let is_cancelled = cancellation_token.load(Ordering::Relaxed);
            if !is_cancelled {
                tx.send(AppEvent::FileSearchResult { query, matches });
            }

            // Update shared state and see if another query is queued.
            let next_query_opt = {
                #[allow(clippy::unwrap_used)]
                let mut st = state.lock().unwrap();

                if let Some(inf) = &st.in_flight {
                    if Arc::ptr_eq(&inf.cancellation_token, &cancellation_token) {
                        st.in_flight = None;
                    }
                }

                st.pending_query.take()
            };

            if let Some(next_query) = next_query_opt {
                let next_token = Arc::new(AtomicBool::new(false));

                {
                    #[allow(clippy::unwrap_used)]
                    let mut st = state.lock().unwrap();
                    st.in_flight = Some(InFlightSearch {
                        query: next_query.clone(),
                        cancellation_token: next_token.clone(),
                    });
                }

                FileSearchManager::spawn_file_search(next_query, search_dir, tx, next_token, state);
            }
        });
    }
}
