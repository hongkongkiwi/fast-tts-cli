use anyhow::{Context, Result};
use clap::{ArgAction, Parser, ValueEnum};
use reqwest::header::{HeaderMap, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use base64::Engine as _;

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum Gender {
    Neutral,
    Male,
    Female,
}

#[derive(Parser, Debug)]
#[command(name = "fast-tts", version, about = "Generate WAV from Google Cloud Text-to-Speech")] 
struct Cli {
    /// Text to synthesize (use quotes)
    text: Option<String>,

    /// Output WAV file path
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

    /// Output sample rate (Hz) for WAV
    #[arg(long = "sample-rate")]
    sample_rate: Option<i32>,

    /// List available voices and exit
    #[arg(long = "list-voices", action = ArgAction::SetTrue)]
    list_voices: bool,

    /// Emit JSON for --list-voices
    #[arg(long = "json", action = ArgAction::SetTrue)]
    json_output: bool,
}

#[derive(Serialize)]
struct SynthesisInput<'a> {
    text: &'a str,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    sample_rate_hertz: Option<i32>,
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

    if let Some(ext) = output.extension() {
        if ext.to_string_lossy().to_lowercase() != "wav" {
            anyhow::bail!("output must end with .wav");
        }
    } else {
        anyhow::bail!("output must end with .wav");
    }

    synthesize_to_wav(
        text,
        &output.to_path_buf(),
        &args.language,
        args.voice.as_deref(),
        args.gender,
        args.rate,
        args.pitch,
        args.sample_rate,
    )
    .await?;

    println!("Wrote {}", output.display());
    Ok(())
}

async fn list_voices(json_output: bool) -> Result<()> {
    let token = fetch_access_token().await?;
    let client = reqwest::Client::new();
    let url = "https://texttospeech.googleapis.com/v1/voices";
    let mut headers = HeaderMap::new();
    headers.insert(AUTHORIZATION, format!("Bearer {}", token).parse()?);

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
            let langs = if v.language_codes.is_empty() { String::from("-") } else { v.language_codes.join(",") };
            let rate = v
                .natural_sample_rate_hertz
                .map(|r| r.to_string())
                .unwrap_or_else(|| "-".into());
            println!("{:<28} {:<7} {:>6} Hz  [{}]", v.name, v.ssml_gender, rate, langs);
        }
    }
    Ok(())
}

async fn synthesize_to_wav(
    text: &str,
    output: &PathBuf,
    language: &str,
    voice: Option<&str>,
    gender: Option<Gender>,
    rate: f32,
    pitch: f32,
    sample_rate: Option<i32>,
) -> Result<()> {
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create output directory: {}", parent.display()))?;
        }
    }

    let token = fetch_access_token().await?;
    let client = reqwest::Client::new();
    let url = "https://texttospeech.googleapis.com/v1/text:synthesize";

    let gender_str = gender.map(|g| match g {
        Gender::Neutral => "NEUTRAL",
        Gender::Male => "MALE",
        Gender::Female => "FEMALE",
    });

    let req_body = SynthesizeRequest {
        input: SynthesisInput { text },
        voice: VoiceSelectionParams {
            language_code: language,
            name: voice,
            ssml_gender: gender_str,
        },
        audio_config: AudioConfig {
            audio_encoding: "LINEAR16",
            speaking_rate: rate,
            pitch,
            sample_rate_hertz: sample_rate,
            enable_legacy_wav_header: false,
        },
    };

    let mut headers = HeaderMap::new();
    headers.insert(AUTHORIZATION, format!("Bearer {}", token).parse()?);
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
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};

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
    struct TokenResp { access_token: String }
    let tr: TokenResp = resp.json().await?;
    Ok(tr.access_token)
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
    struct TokenResp { access_token: String }
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
