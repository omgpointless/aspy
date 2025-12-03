# API Translation System - Implementation Guide

> **Purpose**: Guide for Claude instances working on the translation system. This captures architectural decisions, patterns, and the roadmap for completing streaming integration.

## Quick Context

The translation system converts between API formats (OpenAI ↔ Anthropic) at the HTTP body level. It sits in the proxy pipeline:

```
Client Request (OpenAI) → RequestTranslator → [Proxy Core] → ResponseTranslator → Client Response (OpenAI)
```

**Key distinction from other systems:**
- **Augmentors**: Inject content INTO SSE streams (same format)
- **EventProcessors**: Transform parsed ProxyEvents (post-parsing)
- **Translators**: Convert entire HTTP bodies BETWEEN formats (pre/post proxy)

## Current State

### Fully Integrated
1. **Request translation**: `proxy_handler()` calls `TranslationPipeline::translate_request()`
2. **Buffered response translation**: `handle_buffered_response()` calls `translator.translate_buffered()`

### Infrastructure Ready, Not Integrated
- **Streaming response translation**: `translate_chunk()` and `finalize()` are implemented and tested but NOT called from `handle_streaming_response()`

## File Map

```
src/proxy/translation/
├── mod.rs              # Core traits, ApiFormat enum, TranslationPipeline
├── context.rs          # TranslationContext (carries state request→response)
├── detection.rs        # FormatDetector (path/headers/body analysis)
└── openai/
    ├── mod.rs          # Module exports
    ├── request.rs      # OpenAI → Anthropic request translation
    └── response.rs     # Anthropic → OpenAI response translation (buffered + streaming)
```

## Key Patterns to Follow

### 1. Trait-Based Composition
New translators implement `RequestTranslator` and/or `ResponseTranslator` traits. Register them in `TranslationPipeline::from_config()`:

```rust
// In from_config():
pipeline.register_request_translator(MyNewTranslator::new(model_mapping.clone()));
pipeline.register_response_translator(MyNewResponseTranslator::new(model_mapping));
```

### 2. TranslationContext Carries State
Context is created during request translation and passed to response translation:

```rust
// Request phase:
let (translated_body, ctx) = translator.translate(body, headers)?;

// Response phase (uses same ctx):
let translated_response = translator.translate_buffered(response_body, &ctx)?;
```

For streaming, the context is MUTABLE to track state across chunks:
- `line_buffer`: Handles partial SSE events
- `chunk_index`: Tracks tool call ordering
- `finish_reason`: Captured from `message_delta`

### 3. Model Mapping is Bidirectional
`ModelMapping` converts both directions:
- `to_anthropic("gpt-4")` → `"claude-sonnet-4-20250514"`
- `to_openai("claude-sonnet-4-20250514")` → `"gpt-4-turbo"`

Preserve `original_model` in context to return the exact model name the client sent.

### 4. Format Detection Priority
`FormatDetector` uses multiple signals (see `detection.rs`):
1. **Path** (highest priority): `/v1/chat/completions` = OpenAI, `/v1/messages` = Anthropic
2. **Headers**: Check for format hints
3. **Body structure** (fallback): Look for `messages[].role` vs `messages[].content[].type`

## Streaming Integration TODO

The remaining work is in `src/proxy/mod.rs` → `handle_streaming_response()`.

### What Needs to Change

Currently, chunks flow:
```
Anthropic SSE → forward to client unchanged
```

With translation:
```
Anthropic SSE → translate_chunk() → forward translated to client
                                  → finalize() at end
```

### Implementation Steps

1. **Check if translation needed** (already done):
   ```rust
   let needs_translation = translation_ctx.needs_response_translation();
   ```

2. **Get the response translator**:
   ```rust
   let translator = state.translation
       .get_response_translator(ApiFormat::Anthropic, ApiFormat::OpenAI);
   ```

3. **Wrap chunk processing** (the main change):
   ```rust
   // Inside the while loop processing chunks:
   let chunk_to_send = if needs_translation {
       let translated = translator.translate_chunk(&chunk, &mut translation_ctx)?;
       if translated.is_empty() {
           continue; // Chunk buffered, not ready to send
       }
       Bytes::from(translated)
   } else {
       chunk
   };
   ```

4. **Call finalize after stream ends**:
   ```rust
   // After the while loop:
   if needs_translation {
       if let Some(terminator) = translator.finalize(&translation_ctx) {
           let _ = tx.send(Ok(Bytes::from(terminator))).await;
       }
   }
   ```

### Challenges to Handle

1. **Mutable context across async boundaries**: The `translation_ctx` needs to be mutable inside the spawned task. Currently it's moved into the task - that's fine, just use `&mut translation_ctx` when calling `translate_chunk()`.

2. **Error handling mid-stream**: If `translate_chunk()` fails, decide whether to:
   - Drop the stream (client sees incomplete response)
   - Fall back to raw Anthropic format (may confuse client)
   - Log and continue (skip problematic chunk)

3. **Coordinating with augmentation**: Augmentation currently injects content into Anthropic SSE. If translating, should augmentation happen BEFORE or AFTER translation? Recommendation: augment first (in Anthropic format), then translate.

## Adding a New Format (e.g., Bedrock)

1. **Add enum variant**:
   ```rust
   // mod.rs
   pub enum ApiFormat {
       Anthropic,
       OpenAI,
       Bedrock, // New
   }
   ```

2. **Create submodule**:
   ```
   src/proxy/translation/bedrock/
   ├── mod.rs
   ├── request.rs
   └── response.rs
   ```

3. **Implement traits**:
   ```rust
   pub struct BedrockToAnthropicRequest { /* ... */ }
   impl RequestTranslator for BedrockToAnthropicRequest { /* ... */ }

   pub struct AnthropicToBedrockResponse { /* ... */ }
   impl ResponseTranslator for AnthropicToBedrockResponse { /* ... */ }
   ```

4. **Update FormatDetector**:
   ```rust
   // detection.rs - add Bedrock path/header detection
   if path.contains("/bedrock/") || path.contains("/invoke") {
       return ApiFormat::Bedrock;
   }
   ```

5. **Register in pipeline**:
   ```rust
   // mod.rs TranslationPipeline::from_config()
   pipeline.register_request_translator(bedrock::BedrockToAnthropicRequest::new(...));
   pipeline.register_response_translator(bedrock::AnthropicToBedrockResponse::new(...));
   ```

## Testing Strategy

### Unit Tests (in each module)
- Request translation: Various OpenAI payloads → verify Anthropic output
- Response translation: Various Anthropic responses → verify OpenAI output
- Model mapping: Bidirectional name conversion
- Format detection: Path/header/body detection

### Integration Tests (if added)
- End-to-end: OpenAI client → proxy → Anthropic API → proxy → OpenAI client
- Error cases: Invalid requests, partial responses, network failures

### Manual Testing
```bash
# Enable translation in config
cat >> ~/.config/aspy/config.toml << 'EOF'
[translation]
enabled = true
auto_detect = true
EOF

# Test with curl (OpenAI format)
curl http://localhost:8080/dev-1/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $ANTHROPIC_API_KEY" \
  -d '{
    "model": "gpt-4",
    "messages": [{"role": "user", "content": "Hello!"}],
    "stream": false
  }'
```

## Common Pitfalls

1. **Don't forget `original_model`**: Always preserve it in context so responses return the client's model name, not the translated one.

2. **Temperature scaling**: OpenAI uses 0-2, Anthropic uses 0-1. Scale appropriately in request translation.

3. **Tool call format differences**: OpenAI uses `function` type, Anthropic uses `tool_use`. The `id` fields are compatible.

4. **Stop reason mapping**: `end_turn` → `stop`, `tool_use` → `tool_calls`, `max_tokens` → `length`.

5. **SSE terminator**: OpenAI expects `data: [DONE]\n\n`, Anthropic just ends. Always call `finalize()` for translated streams.

## Questions to Ask the User

If extending this system, clarify:
1. Should streaming translation be a priority? (affects architecture complexity)
2. Which additional formats are needed? (Bedrock, Vertex, Cohere?)
3. Should translation errors fail hard or fall back to passthrough?
4. Is custom model mapping needed beyond defaults?

---

*Last updated: After initial translator implementation (buffered integrated, streaming ready)*
