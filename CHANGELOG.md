## [](https://github.com/omgpointless/aspy/compare/v0.1.0...v) (2025-12-05)

### Features

* add advanced context recovery strategies and improve TOML serialization for clients and providers ([c06230f](https://github.com/omgpointless/aspy/commit/c06230f72b52f963f55fa5b147186ea271db14e3))
* add changelog generation script ([5e4321f](https://github.com/omgpointless/aspy/commit/5e4321fd03d8a56fa0433f81322e9f5c1ed40e65))
* add conditional rules to TagEditor with turn, tool result, and client ID support ([2565f96](https://github.com/omgpointless/aspy/commit/2565f965f1f63ffe69f3cd57e0830aeb22058707))
* add database migration from v2 to v3 and enhance TUI scrolling behavior ([2573396](https://github.com/omgpointless/aspy/commit/2573396e5a8df84465e20a3bbe0919b1f19f5858))
* add event pipeline implementation ([e27df43](https://github.com/omgpointless/aspy/commit/e27df433555320f9982fbe16ce6e523cdcd12ed7))
* add request transformation pipeline and embedding indexer integration ([41c41c8](https://github.com/omgpointless/aspy/commit/41c41c8591fd38458871f13030acf011c49469c4))
* add reverse translation implementations for OpenAI and Anthropic formats ([a6b6a07](https://github.com/omgpointless/aspy/commit/a6b6a072ff7b4cca6534bc9ca5debbc37e124c01))
* add support for Local Ollama provider in claude.sh script ([c8f75ef](https://github.com/omgpointless/aspy/commit/c8f75efe60db8d5d7f798236a5a0a927188326b9))
* add SystemReminderEditor transformer and request transformation framework ([ac8dc7a](https://github.com/omgpointless/aspy/commit/ac8dc7acf0ef77b90ae94865e37474db46a5c769))
* add tag browsing, refine post metadata, and update UI elements ([832e39c](https://github.com/omgpointless/aspy/commit/832e39c3f63ddc8540e16a403e6f8ce00c52364d))
* add token estimation utilities and formatting script ([6c66d47](https://github.com/omgpointless/aspy/commit/6c66d47de97af7ada0d64f0ac5c8ca272bc5817d))
* add transformation and augmentation tracking with token statistics ([d321c94](https://github.com/omgpointless/aspy/commit/d321c94540a339d7790f6cedd48d0e94a3032a14))
* add WSL clipboard support and enhance rule application in SystemReminderEditor ([b1088d7](https://github.com/omgpointless/aspy/commit/b1088d71ca9ce88d11d0f69c1e4da09ba89f8a1b))
* add zoomable interface for TUI components ([6f07d20](https://github.com/omgpointless/aspy/commit/6f07d2082aa8ee7f7823e4b4f914c69e1ab169fe))
* **embeddings:** add OpenAI-compatible remote embedding provider ([040dcfc](https://github.com/omgpointless/aspy/commit/040dcfc1d5d22877172d801bf2188c7b64ce6b1d))
* enhance markdown rendering and structured event overlays and add total_prompts ([9104066](https://github.com/omgpointless/aspy/commit/9104066dfced8470db2797944e96e8ac2910eaa6))
* enhance scrolling and logging with additional context and keybinds ([8690bac](https://github.com/omgpointless/aspy/commit/8690baccb16c071f122626e0607ea54b009111e6))
* enhance session logging and continuity injection ([fc4174e](https://github.com/omgpointless/aspy/commit/fc4174ed541771d1af5a395576b612ab652f62d3))
* enhance user/session context handling and add deep tagging support ([bf88459](https://github.com/omgpointless/aspy/commit/bf88459a522a5658df752476d2abe080605ce223))
* implement bidirectional API translation and complete streaming support ([cb48a5d](https://github.com/omgpointless/aspy/commit/cb48a5d1aa9c85273044914bb275eff909b53a1a))
* implement user-scoped queries and cross-session context recovery ([73ed4de](https://github.com/omgpointless/aspy/commit/73ed4de26f88a8d107b5db854412708b925efae1))
* introduce `claude.sh` launcher and enhance MCP server with context recovery tools ([cc034a0](https://github.com/omgpointless/aspy/commit/cc034a0e94a9a6709005d5c09e7d7e19d3f0d3a9))
* introduce CompactEnhancer for compaction prompt detection and session context injection ([dbfbaed](https://github.com/omgpointless/aspy/commit/dbfbaedd3ae44760af108046414dfb89623c0897))
* introduce file logging with rotation and extended debug tracing ([1b37ee1](https://github.com/omgpointless/aspy/commit/1b37ee16596cd685bc4ec01a5da3e38e354e8950))
* introduce startup registry and hybrid context recovery ([b957f29](https://github.com/omgpointless/aspy/commit/b957f29a98ad2539f333e9d901a9d28d6fe9a6b7))
* introduce TrackedEvent for user/session context and optimize TagEditor ([02e58ae](https://github.com/omgpointless/aspy/commit/02e58aea862ff56a32cc359c70558fc74309cdec))
* overhaul context tools and stats tracking ([284a844](https://github.com/omgpointless/aspy/commit/284a844901b561245f0b868f6e5b8dfd3512b00f))
* **pipeline:** add semantic search with hybrid FTS/vector retrieval ([8e0e4b7](https://github.com/omgpointless/aspy/commit/8e0e4b7975e8af705071b2473cf88b6ea016aa12))
* **proxy:** add bidirectional API translators (OpenAI <-> Anthropic) ([8b5a8cc](https://github.com/omgpointless/aspy/commit/8b5a8cc2e3197d25949a6e9bb351eed8b7c1dce0))
* **proxy:** add bidirectional API translators (OpenAI <-> Anthropic) ([c280a10](https://github.com/omgpointless/aspy/commit/c280a107a1e6c93dd1dee6daa505cee3c484be5d))
* publish deep-dive blog on hybrid search and embedding-powered context recovery ([62c9d9a](https://github.com/omgpointless/aspy/commit/62c9d9afdf8b4a40369d1efacc70a03d10bfff0b))
* streamline API translation and enhance provider configuration ([877d7bd](https://github.com/omgpointless/aspy/commit/877d7bd8062a9de162a65865987945e9549e2330))

### Bug Fixes

* correct argument usage in `truncate_utf8_safe` function ([b48d406](https://github.com/omgpointless/aspy/commit/b48d40678de12d210085b7b9adc7ca6abf0294aa))
## [0.1.0](https://github.com/omgpointless/aspy/compare/v0.1.0-alpha...v0.1.0) (2025-12-02)

### Features

* add mouse scrolling, fix SSE streaming, update dependencies ([154b262](https://github.com/omgpointless/aspy/commit/154b262debf8772dca3a12c3d08c41293e315c55))
* add plugin system, session tracking, stats panels, and clippy cleanup ([1a75030](https://github.com/omgpointless/aspy/commit/1a75030be42659649a820a98b74e0bdff2c92f73))
* improve CLI configuration tool ([a96158c](https://github.com/omgpointless/aspy/commit/a96158c3abce55599cdcf5d096c3d52c8f6a3c8a))
* **presets:** add new layout presets and improve responsiveness ([d418947](https://github.com/omgpointless/aspy/commit/d418947848e9b0836e082358b925888169ffe6e4))
* **proxy:** add multi-client routing with provider backends ([8ed5bb4](https://github.com/omgpointless/aspy/commit/8ed5bb4af3dc3b9354f2cb57e1d3215f6fec365f))
* **theme:** add new "Spy Dark" and "Spy Light" themes with improved Settings integration ([5cc7470](https://github.com/omgpointless/aspy/commit/5cc7470c57a582361f17569a31818885edbda2b7))
* **tui:** add themes, stats view, and responsive layout for v0.1.0 ([8fd9e9f](https://github.com/omgpointless/aspy/commit/8fd9e9f6f67326ec1c702ebb200caad60861cfd5))

### Bug Fixes

* **config:** add multi-client and provider examples to generated config ([460c526](https://github.com/omgpointless/aspy/commit/460c5262ce507f853f6dc3e6b7959f273657fdee))
* **parser:** enable ToolResult correlation for SSE streaming ([9642c7d](https://github.com/omgpointless/aspy/commit/9642c7d3c79ae8a29da25f724b85aa47c9b163c0))
## 0.1.0-alpha (2025-11-26)
