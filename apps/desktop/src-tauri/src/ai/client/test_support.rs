//! Test-only ways to build an [`AiBackend`] and override what it derives from its host. A child
//! of `client`, so it reaches the private fields without widening them.

use super::AiBackend;

impl AiBackend {
    /// [`remote`](Self::remote) for tests outside `ai/` (live smoke tests, evals), which call a
    /// provider directly and so have no consent to check.
    pub(crate) fn remote_for_tests(api_key: String, base_url: String, model: String) -> Self {
        Self::remote(api_key, base_url, model)
    }

    /// [`local`](Self::local) for tests outside `ai/`.
    pub(crate) fn local_for_tests(port: u16) -> Self {
        Self::local(port)
    }

    /// Overrides the host-derived [`replays_chat_reasoning`](Self::replays_chat_reasoning), for a
    /// loopback fixture server standing in for a host that takes reasoning back.
    pub(crate) fn with_chat_reasoning_replay(mut self, replays: bool) -> Self {
        self.replays_chat_reasoning = replays;
        self
    }

    /// Overrides the host-derived [`wants_top_level_cache_marker`](Self::wants_top_level_cache_marker),
    /// for a loopback fixture server standing in for OpenRouter routing to Claude.
    pub(crate) fn with_top_level_cache_marker(mut self, wants: bool) -> Self {
        self.top_level_cache_marker = wants;
        self
    }
}
