use anyhow::{Context, Result};
use base64::Engine as _;
use clap::{ArgAction, Parser, ValueEnum};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
// use std::time::Duration; // reserved for future retries/timeouts

#[cfg(feature = "mcp")]
mod mcp_integration {
    use super::*;
    use anyhow::Result;

    // Import rust-sdk types guarded by feature
    use ::mcp_server::{ByteTransport, Router, Server};
    use axum::{self, response::sse::{Event, Sse}, routing::{get, post}, Json, Router as AxumRouter};
    use tower_service::Service;
    use mcp_spec::content::Content;
    use mcp_spec::handler::{PromptError, ResourceError, ToolError};
    use mcp_spec::prompt::{Prompt, PromptArgument};
    use mcp_spec::protocol::{JsonRpcRequest, JsonRpcResponse, ServerCapabilities};
    use std::convert::Infallible;
    use std::time::Duration;
    use tokio_stream::StreamExt as _;
    use tokio as mcp_tokio;

    pub async fn run_mcp_server(mode: McpMode, addr: Option<String>) -> Result<()> {
        // Build Router implementation backed by our CLI functions
        let router = RouterService(FastTtsRouter);
        let mut server = Server::new(router);

        match mode {
            McpMode::Stdio => {
                let transport = ByteTransport::new(mcp_tokio::io::stdin(), mcp_tokio::io::stdout());
                server.run(transport).await?;
            }
            McpMode::Http | McpMode::Sse => {
                async fn rpc(Json(req): Json<JsonRpcRequest>) -> Result<Json<JsonRpcResponse>, axum::http::StatusCode> {
                    let mut service = mcp_server::router::RouterService(FastTtsRouter);
                    service
                        .call(req)
                        .await
                        .map(Json)
                        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)
                }

                async fn sse() -> Sse<impl futures::Stream<Item = Result<Event, Infallible>>> {
                    let interval = mcp_tokio::time::interval(Duration::from_secs(10));
                    let stream = tokio_stream::wrappers::IntervalStream::new(interval)
                        .map(|_| Ok(Event::default().comment("keepalive")));
                    Sse::new(stream)
                }

                let app: AxumRouter = AxumRouter::new()
                    .route("/rpc", post(rpc))
                    .route("/events", get(sse));

                let bind_addr = addr.unwrap_or_else(|| "127.0.0.1:2024".to_string());
                let listener = mcp_tokio::net::TcpListener::bind(&bind_addr).await?;
                axum::serve(listener, app).await?;
            }
        }
        Ok(())
    }

    use mcp_server::router::{CapabilitiesBuilder, RouterService};

    #[derive(Clone)]
    struct FastTtsRouter;

    impl Router for FastTtsRouter {
        fn name(&self) -> String {
            "fast-tts-cli".to_string()
        }
        fn instructions(&self) -> String {
            "Google Cloud TTS server providing synthesize and listVoices tools.".to_string()
        }
        fn capabilities(&self) -> ServerCapabilities {
            CapabilitiesBuilder::new().with_tools(false).build()
        }
        fn list_tools(&self) -> Vec<mcp_spec::tool::Tool> {
            vec![
                mcp_spec::tool::Tool::new(
                    "synthesize".to_string(),
                    "Synthesize speech and write to a file".to_string(),
                    serde_json::json!({
                        "type": "object",
                        "properties": {
                            "text": {"type": "string"},
                            "output": {"type": "string"},
                            "language": {"type": "string"},
                            "voice": {"type": "string"},
                            "gender": {"type": "string"},
                            "rate": {"type": "number"},
                            "pitch": {"type": "number"},
                            "sampleRate": {"type": "integer"},
                            "encoding": {"type": "string"},
                            "volumeGainDb": {"type": "number"},
                            "effectsProfileId": {"type": "array", "items": {"type": "string"}},
                            "ssml": {"type": "boolean"}
                        },
                        "required": ["text", "output"]
                    }),
                ),
                mcp_spec::tool::Tool::new(
                    "listVoices".to_string(),
                    "List available voices from provider".to_string(),
                    serde_json::json!({ "type": "object", "properties": {"json": {"type": "boolean"}}, "required": [] }),
                ),
            ]
        }

        fn call_tool(
            &self,
            tool_name: &str,
            arguments: serde_json::Value,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<Vec<Content>, ToolError>> + Send + 'static>,
        > {
            let name = tool_name.to_string();
            Box::pin(async move {
                match name.as_str() {
                    "synthesize" => {
                        let text = arguments
                            .get("text")
                            .and_then(|v| v.as_str())
                            .ok_or_else(|| ToolError::InvalidParameters("text required".into()))?
                            .to_string();
                        let output = arguments
                            .get("output")
                            .and_then(|v| v.as_str())
                            .ok_or_else(|| ToolError::InvalidParameters("output required".into()))?
                            .to_string();
                        let language = arguments
                            .get("language")
                            .and_then(|v| v.as_str())
                            .unwrap_or("en-US")
                            .to_string();
                        let voice = arguments
                            .get("voice")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        let gender_opt =
                            arguments.get("gender").and_then(|v| v.as_str()).map(|g| {
                                match g.to_uppercase().as_str() {
                                    "MALE" => Gender::Male,
                                    "FEMALE" => Gender::Female,
                                    _ => Gender::Neutral,
                                }
                            });
                        let rate = arguments
                            .get("rate")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(1.0) as f32;
                        let pitch = arguments
                            .get("pitch")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0) as f32;
                        let sample_rate = arguments
                            .get("sampleRate")
                            .and_then(|v| v.as_i64())
                            .map(|n| n as i32);
                        let encoding = arguments
                            .get("encoding")
                            .and_then(|v| v.as_str())
                            .unwrap_or("LINEAR16");
                        let volume_gain_db = arguments
                            .get("volumeGainDb")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0) as f32;
                        let effects_profile_id: Vec<String> = arguments
                            .get("effectsProfileId")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|x| x.as_str().map(|s| s.to_string()))
                                    .collect()
                            })
                            .unwrap_or_default();
                        let is_ssml = arguments
                            .get("ssml")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);

                        let output_path = PathBuf::from(&output);
                        let enc = super::parse_encoding_from_str(encoding)
                            .map_err(|e| ToolError::ExecutionError(e.to_string()))?;

                        super::synthesize_to_wav(
                            &text,
                            &output_path,
                            &language,
                            voice.as_deref(),
                            gender_opt,
                            rate,
                            pitch,
                            sample_rate,
                            enc,
                            volume_gain_db,
                            &effects_profile_id
                                .iter()
                                .map(|s| s.as_str())
                                .collect::<Vec<_>>(),
                            is_ssml,
                            30_000,
                            2,
                        )
                        .await
                        .map_err(|e| ToolError::ExecutionError(e.to_string()))?;

                        Ok(vec![Content::text(
                            serde_json::json!({"ok": true, "output": output}).to_string(),
                        )])
                    }
                    "listVoices" => {
                        let token = super::fetch_access_token()
                            .await
                            .map_err(|e| ToolError::ExecutionError(e.to_string()))?;
                        let base = super::base_url();
                        let client = super::build_http_client_for_base(&base)
                            .map_err(|e| ToolError::ExecutionError(e.to_string()))?;
                        let url = format!("{base}/v1/voices");
                        let mut headers = HeaderMap::new();
                        let auth_val: reqwest::header::HeaderValue = format!("Bearer {token}")
                            .parse()
                            .map_err(|e: reqwest::header::InvalidHeaderValue| {
                                ToolError::ExecutionError(e.to_string())
                            })?;
                        headers.insert(AUTHORIZATION, auth_val);
                        let resp = client
                            .get(url)
                            .headers(headers)
                            .send()
                            .await
                            .map_err(|e| ToolError::ExecutionError(e.to_string()))?
                            .error_for_status()
                            .map_err(|e| ToolError::ExecutionError(e.to_string()))?;
                        let data: super::ListVoicesResponse = resp
                            .json()
                            .await
                            .map_err(|e| ToolError::ExecutionError(e.to_string()))?;
                        Ok(vec![Content::text(
                            serde_json::to_string(&data).unwrap_or_else(|_| "{}".into()),
                        )])
                    }
                    _ => Err(ToolError::NotFound(format!("Tool {} not found", name))),
                }
            })
        }

        fn list_resources(&self) -> Vec<mcp_spec::resource::Resource> {
            vec![]
        }
        fn read_resource(
            &self,
            _uri: &str,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<String, ResourceError>> + Send + 'static>,
        > {
            Box::pin(async { Err(ResourceError::NotFound("not supported".into())) })
        }
        fn list_prompts(&self) -> Vec<Prompt> {
            vec![]
        }
        fn get_prompt(
            &self,
            _prompt_name: &str,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<String, PromptError>> + Send + 'static>,
        > {
            Box::pin(async { Ok(String::new()) })
        }
    }
}
#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum Gender {
    Neutral,
    Male,
    Female,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum Provider {
    Google,
    Openai,
    Elevenlabs,
    Deepgram,
    Polly,
    Azure,
    Hume,
    Listnr,
    Murf,
    Gemini,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "SCREAMING_SNAKE_CASE")]
enum AudioEncoding {
    Linear16,
    Mp3,
    OggOpus,
    Mulaw,
    Alaw,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum McpMode {
    Stdio,
    Sse,
    Http,
}

impl AudioEncoding {
    fn api_str(&self) -> &'static str {
        match self {
            AudioEncoding::Linear16 => "LINEAR16",
            AudioEncoding::Mp3 => "MP3",
            AudioEncoding::OggOpus => "OGG_OPUS",
            AudioEncoding::Mulaw => "MULAW",
            AudioEncoding::Alaw => "ALAW",
        }
    }

    fn file_extension(&self) -> &'static str {
        match self {
            AudioEncoding::Linear16 | AudioEncoding::Mulaw | AudioEncoding::Alaw => "wav",
            AudioEncoding::Mp3 => "mp3",
            AudioEncoding::OggOpus => "ogg",
        }
    }
}

#[derive(Parser, Debug)]
#[command(
    name = "fast-tts",
    version,
    about = "Generate audio from Google Cloud Text-to-Speech"
)]
struct Cli {
    /// Text to synthesize (use quotes)
    text: Option<String>,

    /// Output file path (matches encoding)
    output: Option<PathBuf>,

    /// BCP-47 language code (e.g. en-US)
    #[arg(short = 'l', long = "language", default_value = "en-US")]
    language: String,

    /// Specific voice name (e.g. en-US-Neural2-F)
    #[arg(short = 'v', long = "voice")]
    voice: Option<String>,

    /// Preferred voice gender
    #[arg(long = "gender", value_enum)]
    gender: Option<Gender>,

    /// Speaking rate multiplier (0.25–4.0)
    #[arg(long = "rate", default_value_t = 1.0)]
    rate: f32,

    /// Pitch in semitones (-20.0–20.0)
    #[arg(long = "pitch", default_value_t = 0.0)]
    pitch: f32,

    /// Output sample rate (Hz)
    #[arg(long = "sample-rate")]
    sample_rate: Option<i32>,

    /// Audio encoding (LINEAR16, MP3, OGG_OPUS, MULAW, ALAW)
    #[arg(
        long = "encoding",
        value_enum,
        default_value = "LINEAR16",
        ignore_case = true
    )]
    encoding: AudioEncoding,

    /// Volume gain in dB (-96.0–16.0)
    #[arg(long = "volume", default_value_t = 0.0)]
    volume_gain_db: f32,

    /// Audio effects profile id(s) (comma-separated or repeat flag)
    #[arg(long = "effects-profile", num_args = 0.., value_delimiter = ',')]
    effects_profile_id: Vec<String>,

    /// Treat input as SSML instead of plaintext
    #[arg(long = "ssml", action = ArgAction::SetTrue)]
    ssml: bool,

    /// Use config file (YAML or JSON) for bulk synthesis
    #[arg(long = "config", value_name = "FILE")]
    config_path: Option<PathBuf>,

    /// TTS provider (future: more providers). Only 'google' works now.
    #[arg(long = "provider", value_enum, default_value = "google")]
    provider: Provider,

    /// List available voices and exit
    #[arg(long = "list-voices", action = ArgAction::SetTrue)]
    list_voices: bool,

    /// Emit JSON for --list-voices
    #[arg(long = "json", action = ArgAction::SetTrue)]
    json_output: bool,

    /// Request timeout in milliseconds
    #[arg(long = "timeout", default_value_t = 30_000)]
    timeout_ms: u64,

    /// Number of retries for transient failures
    #[arg(long = "retries", default_value_t = 2)]
    retries: usize,

    /// Run as Model Context Protocol server (stdio, sse, http)
    #[arg(long = "mcp-mode", value_enum)]
    mcp_mode: Option<McpMode>,

    /// Address or URL for MCP SSE/HTTP (e.g. 127.0.0.1:2024 or http://127.0.0.1:2024)
    #[arg(long = "mcp-addr")]
    mcp_addr: Option<String>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum SynthesisInput<'a> {
    #[serde(rename_all = "camelCase")]
    Text { text: &'a str },
    #[serde(rename_all = "camelCase")]
    Ssml { ssml: &'a str },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VoiceSelectionParams<'a> {
    language_code: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ssml_gender: Option<&'a str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AudioConfig<'a> {
    audio_encoding: &'a str,
    speaking_rate: f32,
    pitch: f32,
    volume_gain_db: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    sample_rate_hertz: Option<i32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    effects_profile_id: Vec<&'a str>,
    enable_legacy_wav_header: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SynthesizeRequest<'a> {
    input: SynthesisInput<'a>,
    voice: VoiceSelectionParams<'a>,
    audio_config: AudioConfig<'a>,
}

#[derive(Deserialize)]
struct SynthesizeResponse {
    audio_content: String,
}

#[derive(Deserialize, Serialize)]
struct ListVoicesResponse {
    voices: Vec<Voice>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Voice {
    name: String,
    language_codes: Vec<String>,
    ssml_gender: String,
    natural_sample_rate_hertz: Option<i32>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Cli::parse();

    // If running in MCP server mode, start the server and exit.
    if let Some(_mode) = args.mcp_mode {
        #[cfg(feature = "mcp")]
        {
            return mcp_integration::run_mcp_server(_mode, args.mcp_addr).await;
        }
        #[cfg(not(feature = "mcp"))]
        {
            anyhow::bail!(
                "This binary was built without 'mcp' feature. Rebuild with: cargo run --features mcp -- --mcp-mode stdio"
            );
        }
    }

    if let Some(cfg_path) = &args.config_path {
        run_bulk_from_config(cfg_path, args.timeout_ms, args.retries).await?;
        return Ok(());
    }

    if args.list_voices {
        list_voices(args.json_output).await?;
        return Ok(());
    }

    let text = args
        .text
        .as_deref()
        .context("text and output are required unless --list-voices is used")?;
    let output = args
        .output
        .as_deref()
        .context("text and output are required unless --list-voices is used")?;

    validate_output_extension(output, args.encoding)?;

    match args.provider {
        Provider::Google => {
            synthesize_to_wav(
                text,
                output,
                &args.language,
                args.voice.as_deref(),
                args.gender,
                args.rate,
                args.pitch,
                args.sample_rate,
                args.encoding,
                args.volume_gain_db,
                &args
                    .effects_profile_id
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>(),
                args.ssml,
                args.timeout_ms,
                args.retries,
            )
            .await?;
        }
        Provider::Gemini => {
            synthesize_gemini(
                text,
                output,
                args.voice.as_deref(),
                args.encoding,
            )
            .await?;
        }
        Provider::Azure => {
            synthesize_azure(
                text,
                output,
                &args.language,
                args.voice.as_deref(),
                args.encoding,
                args.sample_rate,
            )
            .await?;
        }
        Provider::Openai => {
            synthesize_openai(text, output, args.voice.as_deref(), args.encoding).await?;
        }
        Provider::Elevenlabs => {
            synthesize_elevenlabs(
                text,
                output,
                args.voice.as_deref(),
                args.encoding,
                std::env::var("ELEVENLABS_MODEL_ID").ok().as_deref(),
            )
            .await?;
        }
        Provider::Deepgram => {
            synthesize_deepgram(
                text,
                output,
                args.voice.as_deref(),
                args.encoding,
                std::env::var("DEEPGRAM_TTS_MODEL").ok().as_deref(),
            )
            .await?;
        }
        Provider::Polly => {
            #[cfg(feature = "polly")]
            {
                synthesize_polly(text, output, args.voice.as_deref(), args.encoding).await?;
            }
            #[cfg(not(feature = "polly"))]
            {
                anyhow::bail!("Amazon Polly support requires --features polly");
            }
        }
        Provider::Hume | Provider::Listnr | Provider::Murf => {
            anyhow::bail!(
                "provider {:?} not yet implemented. Please open an issue with API details.",
                args.provider
            );
        }
    }

    println!("Wrote {}", output.display());
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BulkDefaults {
    language: Option<String>,
    voice: Option<String>,
    gender: Option<String>,
    rate: Option<f32>,
    pitch: Option<f32>,
    sample_rate: Option<i32>,
    encoding: Option<String>,
    volume_gain_db: Option<f32>,
    effects_profile_id: Option<Vec<String>>,
    ssml: Option<bool>,
    output_dir: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BulkItem {
    text: String,
    output: Option<String>,
    language: Option<String>,
    voice: Option<String>,
    gender: Option<String>,
    rate: Option<f32>,
    pitch: Option<f32>,
    sample_rate: Option<i32>,
    encoding: Option<String>,
    volume_gain_db: Option<f32>,
    effects_profile_id: Option<Vec<String>>,
    ssml: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BulkConfig {
    defaults: Option<BulkDefaults>,
    items: Vec<BulkItem>,
}

async fn run_bulk_from_config(path: &PathBuf, timeout_ms: u64, retries: usize) -> Result<()> {
    let data = fs::read_to_string(path)
        .with_context(|| format!("failed to read config: {}", path.display()))?;
    let is_yaml = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| matches!(e.to_lowercase().as_str(), "yml" | "yaml"))
        .unwrap_or(false);

    let cfg: BulkConfig = if is_yaml {
        serde_yaml::from_str(&data)?
    } else {
        serde_json::from_str(&data)?
    };

    let defaults = cfg.defaults.unwrap_or(BulkDefaults {
        language: Some("en-US".to_string()),
        voice: None,
        gender: None,
        rate: Some(1.0),
        pitch: Some(0.0),
        sample_rate: None,
        encoding: Some("LINEAR16".to_string()),
        volume_gain_db: Some(0.0),
        effects_profile_id: Some(vec![]),
        ssml: Some(false),
        output_dir: None,
    });

    for (idx, item) in cfg.items.iter().enumerate() {
        let language = item
            .language
            .as_ref()
            .or(defaults.language.as_ref())
            .cloned()
            .unwrap_or_else(|| "en-US".into());
        let voice = item.voice.as_ref().or(defaults.voice.as_ref()).cloned();
        let gender = item.gender.as_ref().or(defaults.gender.as_ref()).map(|g| {
            match g.to_uppercase().as_str() {
                "MALE" => Gender::Male,
                "FEMALE" => Gender::Female,
                _ => Gender::Neutral,
            }
        });
        let rate = item.rate.or(defaults.rate).unwrap_or(1.0);
        let pitch = item.pitch.or(defaults.pitch).unwrap_or(0.0);
        let sample_rate = item.sample_rate.or(defaults.sample_rate);
        let encoding = item
            .encoding
            .as_ref()
            .or(defaults.encoding.as_ref())
            .cloned()
            .unwrap_or_else(|| "LINEAR16".into());
        let volume_gain_db = item
            .volume_gain_db
            .or(defaults.volume_gain_db)
            .unwrap_or(0.0);
        let effects_profile_id: Vec<String> = item
            .effects_profile_id
            .clone()
            .or(defaults.effects_profile_id.clone())
            .unwrap_or_default();
        let is_ssml = item.ssml.or(defaults.ssml).unwrap_or(false);

        // Determine output path
        let output = if let Some(o) = &item.output {
            PathBuf::from(o)
        } else if let Some(dir) = &defaults.output_dir {
            let ext = match encoding.to_uppercase().as_str() {
                "LINEAR16" | "MULAW" | "ALAW" => "wav",
                "MP3" => "mp3",
                "OGG_OPUS" => "ogg",
                _ => "bin",
            };
            PathBuf::from(dir).join(format!("item_{}.{}", idx + 1, ext))
        } else {
            let ext = match encoding.to_uppercase().as_str() {
                "LINEAR16" | "MULAW" | "ALAW" => "wav",
                "MP3" => "mp3",
                "OGG_OPUS" => "ogg",
                _ => "bin",
            };
            PathBuf::from(format!("item_{}.{}", idx + 1, ext))
        };

        validate_output_extension(&output, parse_encoding_from_str(&encoding)?)?;

        // For now, bulk uses Google flow; extend with per-provider if needed
        synthesize_to_wav(
            &item.text,
            &output,
            &language,
            voice.as_deref(),
            gender,
            rate,
            pitch,
            sample_rate,
            parse_encoding_from_str(&encoding)?,
            volume_gain_db,
            &effects_profile_id
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>(),
            is_ssml,
            timeout_ms,
            retries,
        )
        .await?;

        println!("Wrote {}", output.display());
    }

    Ok(())
}

// Provider parsing removed (Google only)
fn base_url() -> String {
    std::env::var("FAST_TTS_BASE_URL")
        .unwrap_or_else(|_| "https://texttospeech.googleapis.com".to_string())
}

fn build_http_client_for_base(base: &str) -> Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder();
    if base.contains("127.0.0.1") || base.contains("localhost") {
        builder = builder.no_proxy();
    }
    Ok(builder.build()?)
}

async fn list_voices(json_output: bool) -> Result<()> {
    let token = fetch_access_token().await?;
    let base = base_url();
    let client = build_http_client_for_base(&base)?;
    let url = format!("{base}/v1/voices");
    let mut headers = HeaderMap::new();
    headers.insert(AUTHORIZATION, format!("Bearer {token}").parse()?);

    let resp = client
        .get(url)
        .headers(headers)
        .send()
        .await?
        .error_for_status()?;

    let data: ListVoicesResponse = resp.json().await?;

    if json_output {
        println!("{}", serde_json::to_string_pretty(&data)?);
    } else {
        for v in &data.voices {
            let langs = if v.language_codes.is_empty() {
                String::from("-")
            } else {
                v.language_codes.join(",")
            };
            let rate = v
                .natural_sample_rate_hertz
                .map(|r| r.to_string())
                .unwrap_or_else(|| "-".into());
            println!(
                "{:<28} {:<7} {:>6} Hz  [{}]",
                v.name, v.ssml_gender, rate, langs
            );
        }
    }
    Ok(())
}

fn validate_output_extension(output: &Path, encoding: AudioEncoding) -> Result<()> {
    let want_ext = encoding.file_extension();
    match output
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase())
    {
        Some(ref ext) if ext == want_ext => Ok(()),
        Some(ext) => anyhow::bail!(
            "output extension .{} does not match encoding {} (expected .{})",
            ext,
            encoding.api_str(),
            want_ext
        ),
        None => anyhow::bail!(
            "output must have .{} extension for encoding {}",
            want_ext,
            encoding.api_str()
        ),
    }
}

async fn synthesize_openai(
    text: &str,
    output: &Path,
    voice: Option<&str>,
    encoding: AudioEncoding,
) -> Result<()> {
    let api_key = std::env::var("OPENAI_API_KEY")
        .context("OPENAI_API_KEY is required for provider openai")?;
    let model = std::env::var("OPENAI_TTS_MODEL").unwrap_or_else(|_| "gpt-4o-mini-tts".to_string());
    let voice_name = voice.unwrap_or("alloy");
    let out_format = match encoding {
        AudioEncoding::Mp3 => "mp3",
        AudioEncoding::OggOpus => "opus",
        _ => "wav",
    };
    let client = reqwest::Client::new();
    let url = "https://api.openai.com/v1/audio/speech";
    let resp = client
        .post(url)
        .bearer_auth(api_key)
        .json(&serde_json::json!({
            "model": model,
            "voice": voice_name,
            "input": text,
            "format": out_format
        }))
        .send()
        .await?
        .error_for_status()?;
    let bytes = resp.bytes().await?;
    if let Some(parent) = output.parent() { if !parent.as_os_str().is_empty() { fs::create_dir_all(parent)?; } }
    fs::write(output, &bytes)?;
    Ok(())
}

async fn synthesize_azure(
    text: &str,
    output: &Path,
    language: &str,
    voice: Option<&str>,
    encoding: AudioEncoding,
    sample_rate: Option<i32>,
) -> Result<()> {
    let key = std::env::var("AZURE_SPEECH_KEY").context("AZURE_SPEECH_KEY is required for provider azure")?;
    let region = std::env::var("AZURE_SPEECH_REGION").context("AZURE_SPEECH_REGION is required for provider azure")?;
    let voice_name = voice.unwrap_or(match language {
        // sensible defaults by locale
        l if l.starts_with("en-US") => "en-US-JennyNeural",
        l if l.starts_with("en-GB") => "en-GB-LibbyNeural",
        _ => "en-US-JennyNeural",
    });
    let format = match (encoding, sample_rate) {
        (AudioEncoding::Mp3, Some(_)) => "audio-48khz-192kbitrate-mono-mp3".to_string(),
        (AudioEncoding::Mp3, None) => "audio-24khz-160kbitrate-mono-mp3".to_string(),
        (AudioEncoding::OggOpus, _) => "ogg-48khz-16bit-mono-opus".to_string(),
        (AudioEncoding::Linear16, Some(sr)) if sr >= 48000 => "riff-48khz-16bit-mono-pcm".to_string(),
        (AudioEncoding::Linear16, _) => "riff-24khz-16bit-mono-pcm".to_string(),
        (AudioEncoding::Mulaw, _) => "mulaw-8khz-8bit-mono".to_string(),
        (AudioEncoding::Alaw, _) => "alaw-8khz-8bit-mono".to_string(),
    };
    let ssml = format!(
        "<speak version=\"1.0\" xml:lang=\"{lang}\"><voice xml:lang=\"{lang}\" name=\"{voice}\">{text}</voice></speak>",
        lang = language,
        voice = voice_name,
        text = htmlescape::encode_minimal(text)
    );
    let url = format!("https://{region}.tts.speech.microsoft.com/cognitiveservices/v1");
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .header("Ocp-Apim-Subscription-Key", key)
        .header("X-Microsoft-OutputFormat", format)
        .header(CONTENT_TYPE, "application/ssml+xml")
        .header("User-Agent", "fast-tts-cli")
        .body(ssml)
        .send()
        .await?
        .error_for_status()?;
    let bytes = resp.bytes().await?;
    if let Some(parent) = output.parent() { if !parent.as_os_str().is_empty() { fs::create_dir_all(parent)?; } }
    fs::write(output, &bytes)?;
    Ok(())
}

async fn synthesize_elevenlabs(
    text: &str,
    output: &Path,
    voice: Option<&str>,
    encoding: AudioEncoding,
    model_id: Option<&str>,
) -> Result<()> {
    let api_key = std::env::var("ELEVENLABS_API_KEY")
        .context("ELEVENLABS_API_KEY is required for provider elevenlabs")?;
    let voice_id = voice.unwrap_or("Rachel");
    let model = model_id.unwrap_or("eleven_multilingual_v2");
    let format = match encoding { AudioEncoding::Mp3 => "mp3", AudioEncoding::OggOpus => "ogg", _ => "wav" };
    let url = format!("https://api.elevenlabs.io/v1/text-to-speech/{voice_id}");
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .header("xi-api-key", api_key)
        .header(CONTENT_TYPE, "application/json")
        .json(&serde_json::json!({
            "text": text,
            "model_id": model,
            "voice_settings": {"stability": 0.5, "similarity_boost": 0.5},
            "output_format": format
        }))
        .send()
        .await?
        .error_for_status()?;
    let bytes = resp.bytes().await?;
    if let Some(parent) = output.parent() { if !parent.as_os_str().is_empty() { fs::create_dir_all(parent)?; } }
    fs::write(output, &bytes)?;
    Ok(())
}

async fn synthesize_deepgram(
    text: &str,
    output: &Path,
    voice: Option<&str>,
    encoding: AudioEncoding,
    model_id: Option<&str>,
) -> Result<()> {
    let api_key = std::env::var("DEEPGRAM_API_KEY")
        .context("DEEPGRAM_API_KEY is required for provider deepgram")?;
    let model = model_id.unwrap_or("aura-asteria-en");
    let voice_name = voice.unwrap_or("aura-asteria-en");
    let format = match encoding { AudioEncoding::Mp3 => "mp3", AudioEncoding::OggOpus => "opus", _ => "wav" };
    let url = "https://api.deepgram.com/v1/speak";
    let client = reqwest::Client::new();
    let resp = client
        .post(url)
        .header("Authorization", format!("Token {api_key}"))
        .query(&[("model", model), ("voice", voice_name), ("format", format)])
        .body(text.to_string())
        .send()
        .await?
        .error_for_status()?;
    let bytes = resp.bytes().await?;
    if let Some(parent) = output.parent() { if !parent.as_os_str().is_empty() { fs::create_dir_all(parent)?; } }
    fs::write(output, &bytes)?;
    Ok(())
}

async fn synthesize_gemini(
    text: &str,
    output: &Path,
    voice: Option<&str>,
    encoding: AudioEncoding,
) -> Result<()> {
    let api_key = std::env::var("GEMINI_API_KEY")
        .context("GEMINI_API_KEY is required for provider gemini")?;
    // Allow overriding the model; default to a fast, generally-available model
    let model = std::env::var("GEMINI_TTS_MODEL")
        .unwrap_or_else(|_| "gemini-1.5-flash-latest".to_string());

    let format = match encoding {
        AudioEncoding::Mp3 => "mp3",
        AudioEncoding::OggOpus => "ogg",
        AudioEncoding::Linear16 => "wav",
        AudioEncoding::Mulaw | AudioEncoding::Alaw => {
            anyhow::bail!(
                "Gemini speech does not support {} encoding; use MP3/OGG_OPUS/LINEAR16",
                encoding.api_str()
            )
        }
    };

    #[derive(serde::Serialize)]
    struct AudioPart<'a> {
        voice: Option<&'a str>,
        format: &'a str,
    }

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        model, api_key
    );

    // Build request payload using Gemini generateContent structure
    let request_body = serde_json::json!({
        "contents": [
            {
                "role": "user",
                "parts": [
                    { "text": text },
                    { "audio": AudioPart { voice, format } }
                ]
            }
        ]
    });

    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .header(CONTENT_TYPE, "application/json")
        .json(&request_body)
        .send()
        .await?
        .error_for_status()?;

    #[derive(Deserialize)]
    struct GeminiAudio {
        data: String,
        #[allow(dead_code)]
        #[serde(rename = "mimeType")]
        mime_type: Option<String>,
    }
    #[derive(Deserialize)]
    struct GeminiPartResp {
        #[serde(default)]
        audio: Option<GeminiAudio>,
        #[allow(dead_code)]
        #[serde(default)]
        text: Option<String>,
    }
    #[derive(Deserialize)]
    struct GeminiContentResp { parts: Vec<GeminiPartResp> }
    #[derive(Deserialize)]
    struct GeminiCandidate { content: GeminiContentResp }
    #[derive(Deserialize)]
    struct GeminiResponse { candidates: Vec<GeminiCandidate> }

    let gr: GeminiResponse = resp.json().await?;

    // Find first audio part with data
    let audio_b64 = gr
        .candidates
        .into_iter()
        .flat_map(|c| c.content.parts)
        .find_map(|p| p.audio.map(|a| a.data))
        .context("Gemini response did not include audio data")?;

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(audio_b64)
        .context("failed decoding audio data from Gemini response")?;

    if let Some(parent) = output.parent() { if !parent.as_os_str().is_empty() { fs::create_dir_all(parent)?; } }
    fs::write(output, bytes)?;
    Ok(())
}

#[cfg(feature = "polly")]
async fn synthesize_polly(
    text: &str,
    output: &Path,
    voice: Option<&str>,
    encoding: AudioEncoding,
) -> Result<()> {
    use aws_sdk_polly::types::{Engine, OutputFormat, VoiceId};
    let config = aws_config::load_from_env().await;
    let client = aws_sdk_polly::Client::new(&config);
    let voice_id = voice.unwrap_or("Joanna");
    let output_format = match encoding { AudioEncoding::Mp3 => OutputFormat::Mp3, AudioEncoding::OggOpus => OutputFormat::OggVorbis, _ => OutputFormat::Pcm };
    let resp = client
        .synthesize_speech()
        .set_text(Some(text.to_string()))
        .set_voice_id(Some(VoiceId::from(voice_id)))
        .set_output_format(Some(output_format))
        .set_engine(Some(Engine::Neural))
        .send()
        .await?;
    let data = resp.audio_stream.unwrap().into_bytes().collect::<Result<Vec<_>, _>>()?;
    if let Some(parent) = output.parent() { if !parent.as_os_str().is_empty() { fs::create_dir_all(parent)?; } }
    fs::write(output, data)?;
    Ok(())
}
#[allow(clippy::too_many_arguments)]
async fn synthesize_to_wav(
    text: &str,
    output: &Path,
    language: &str,
    voice: Option<&str>,
    gender: Option<Gender>,
    rate: f32,
    pitch: f32,
    sample_rate: Option<i32>,
    encoding: AudioEncoding,
    volume_gain_db: f32,
    effects_profile_id: &[&str],
    is_ssml: bool,
    _timeout_ms: u64,
    _retries: usize,
) -> Result<()> {
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create output directory: {}", parent.display())
            })?;
        }
    }

    let token = fetch_access_token().await?;
    let base = base_url();
    let client = build_http_client_for_base(&base)?;
    let url = format!("{base}/v1/text:synthesize");

    let gender_str = gender.map(|g| match g {
        Gender::Neutral => "NEUTRAL",
        Gender::Male => "MALE",
        Gender::Female => "FEMALE",
    });

    let req_body = SynthesizeRequest {
        input: if is_ssml {
            SynthesisInput::Ssml { ssml: text }
        } else {
            SynthesisInput::Text { text }
        },
        voice: VoiceSelectionParams {
            language_code: language,
            name: voice,
            ssml_gender: gender_str,
        },
        audio_config: AudioConfig {
            audio_encoding: encoding.api_str(),
            speaking_rate: rate,
            pitch,
            volume_gain_db,
            sample_rate_hertz: sample_rate,
            effects_profile_id: effects_profile_id.to_vec(),
            enable_legacy_wav_header: false,
        },
    };

    let mut headers = HeaderMap::new();
    headers.insert(AUTHORIZATION, format!("Bearer {token}").parse()?);
    headers.insert(CONTENT_TYPE, "application/json".parse()?);

    let resp = client
        .post(url)
        .headers(headers)
        .json(&req_body)
        .send()
        .await?
        .error_for_status()?;

    let data: SynthesizeResponse = resp.json().await?;
    let audio = base64::engine::general_purpose::STANDARD.decode(data.audio_content)?;
    fs::write(output, audio).with_context(|| format!("failed to write {}", output.display()))?;
    Ok(())
}

async fn fetch_access_token() -> Result<String> {
    if let Ok(token) = std::env::var("FAST_TTS_TOKEN") {
        if !token.trim().is_empty() {
            return Ok(token);
        }
    }
    // Supports two common methods:
    // 1) GOOGLE_APPLICATION_CREDENTIALS pointing at a service account JSON key
    // 2) gcloud application-default credentials at well-known path
    if let Ok(path) = std::env::var("GOOGLE_APPLICATION_CREDENTIALS") {
        return fetch_token_from_service_account(PathBuf::from(path)).await;
    }

    if let Some(path) = default_adc_path() {
        if path.exists() {
            if let Ok(token) = fetch_token_from_adc(path).await {
                return Ok(token);
            }
        }
    }

    anyhow::bail!(
        "No Google credentials found. Set GOOGLE_APPLICATION_CREDENTIALS or run 'gcloud auth application-default login'"
    );
}

#[derive(Deserialize)]
struct ServiceAccountKey {
    client_email: String,
    private_key: String,
    token_uri: Option<String>,
}

async fn fetch_token_from_service_account(path: PathBuf) -> Result<String> {
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};

    let key_data = fs::read_to_string(&path)
        .with_context(|| format!("failed to read service account key: {}", path.display()))?;
    let key: ServiceAccountKey = serde_json::from_str(&key_data)?;

    let scope = "https://www.googleapis.com/auth/cloud-platform";
    let token_uri = key
        .token_uri
        .unwrap_or_else(|| "https://oauth2.googleapis.com/token".to_string());

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as usize;

    #[derive(Serialize)]
    struct Claims<'a> {
        iss: &'a str,
        scope: &'a str,
        aud: &'a str,
        exp: usize,
        iat: usize,
    }

    let claims = Claims {
        iss: &key.client_email,
        scope,
        aud: &token_uri,
        exp: now + 3600,
        iat: now,
    };

    let mut header = Header::new(Algorithm::RS256);
    header.typ = Some("JWT".to_string());

    let encoding_key = EncodingKey::from_rsa_pem(key.private_key.as_bytes())
        .context("invalid RSA private key in service account")?;
    let jwt = encode(&header, &claims, &encoding_key)?;

    let client = reqwest::Client::new();
    let resp = client
        .post(&token_uri)
        .form(&serde_json::json!({
            "grant_type": "urn:ietf:params:oauth:grant-type:jwt-bearer",
            "assertion": jwt,
        }))
        .send()
        .await?
        .error_for_status()?;
    #[derive(Deserialize)]
    struct TokenResp {
        access_token: String,
    }
    let tr: TokenResp = resp.json().await?;
    Ok(tr.access_token)
}

fn parse_encoding_from_str(s: &str) -> Result<AudioEncoding> {
    match s.trim().to_uppercase().as_str() {
        "LINEAR16" => Ok(AudioEncoding::Linear16),
        "MP3" => Ok(AudioEncoding::Mp3),
        "OGG_OPUS" => Ok(AudioEncoding::OggOpus),
        "MULAW" => Ok(AudioEncoding::Mulaw),
        "ALAW" => Ok(AudioEncoding::Alaw),
        other => anyhow::bail!("unsupported encoding: {other}"),
    }
}

async fn fetch_token_from_adc(path: PathBuf) -> Result<String> {
    // Application Default Credentials created by gcloud have refresh_token, client_id, client_secret
    let data = fs::read_to_string(&path)
        .with_context(|| format!("failed to read ADC file: {}", path.display()))?;
    #[derive(Deserialize)]
    struct AdcFile {
        client_id: String,
        client_secret: String,
        refresh_token: String,
        #[allow(dead_code)]
        r#type: String,
    }
    let adc: AdcFile = serde_json::from_str(&data)?;

    let client = reqwest::Client::new();
    let resp = client
        .post("https://oauth2.googleapis.com/token")
        .form(&serde_json::json!({
            "grant_type": "refresh_token",
            "client_id": adc.client_id,
            "client_secret": adc.client_secret,
            "refresh_token": adc.refresh_token,
        }))
        .send()
        .await?
        .error_for_status()?;
    #[derive(Deserialize)]
    struct TokenResp {
        access_token: String,
    }
    let tr: TokenResp = resp.json().await?;
    Ok(tr.access_token)
}

fn default_adc_path() -> Option<PathBuf> {
    // macOS/Linux default location
    if let Some(home) = dirs::home_dir() {
        let p = home
            .join(".config")
            .join("gcloud")
            .join("application_default_credentials.json");
        return Some(p);
    }
    None
}
