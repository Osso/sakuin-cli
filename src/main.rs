#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

mod grpc_web;

pub mod proto {
    tonic::include_proto!("manga.v1");
}

use grpc_web::GrpcWebClient;
use proto::{
    GetMangaRequest, GetStatsRequest, GetUserStatsRequest, ListMangaRequest, ListUserMangaRequest,
    Manga, RateMangaRequest, ReadingStatus, SearchMangaRequest, SetProgressRequest,
    SetReadingStatusRequest,
};

#[derive(Parser)]
#[command(name = "sakuin", about = "CLI client for Sakuin manga database")]
struct Cli {
    /// Server URL
    #[arg(short, long, default_value = "https://sakuin.org")]
    server: String,

    /// API token (or set in config)
    #[arg(short, long)]
    token: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Get database statistics
    Stats,
    /// Search for manga
    Search {
        query: String,
        #[arg(short, long, default_value = "10")]
        limit: i32,
    },
    /// Get manga by ID
    Get { id: i64 },
    /// List manga
    List {
        #[arg(short, long, default_value = "1")]
        page: i32,
        #[arg(short = 'n', long, default_value = "20")]
        per_page: i32,
    },
    /// List your tracked manga (requires auth)
    ListMine {
        /// Filter by status: planning, reading, completed, on_hold, dropped
        status: Option<String>,
    },
    /// Set reading status for a manga (requires auth)
    Track {
        manga_id: i64,
        /// Status: planning, reading, completed, on_hold, dropped, not_interested
        status: String,
    },
    /// Set progress for a manga (requires auth)
    Progress { manga_id: i64, progress: String },
    /// Rate a manga 1-10 (requires auth)
    Rate { manga_id: i64, score: i32 },
    /// Get your stats (requires auth)
    StatsMine,
    /// Configure server and token
    Config {
        #[arg(long)]
        server: Option<String>,
        #[arg(long)]
        token: Option<String>,
    },
}

#[derive(Serialize, Deserialize, Default)]
struct Config {
    server: Option<String>,
    token: Option<String>,
}

#[cfg_attr(coverage_nightly, coverage(off))]
fn config_path() -> PathBuf {
    directories::ProjectDirs::from("", "", "sakuin-cli")
        .map(|d| d.config_dir().join("config.json"))
        .unwrap_or_else(|| PathBuf::from("config.json"))
}

#[cfg_attr(coverage_nightly, coverage(off))]
fn load_config() -> Config {
    let path = config_path();
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

#[cfg_attr(coverage_nightly, coverage(off))]
fn save_config(config: &Config) -> Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(config)?)?;
    Ok(())
}

fn parse_status(s: &str) -> Result<ReadingStatus> {
    match s.to_lowercase().as_str() {
        "planning" => Ok(ReadingStatus::Planning),
        "reading" => Ok(ReadingStatus::Reading),
        "completed" => Ok(ReadingStatus::Completed),
        "on_hold" | "onhold" => Ok(ReadingStatus::OnHold),
        "dropped" => Ok(ReadingStatus::Dropped),
        "not_interested" | "notinterested" => Ok(ReadingStatus::NotInterested),
        _ => anyhow::bail!(
            "Invalid status: {}. Use: planning, reading, completed, on_hold, dropped, not_interested",
            s
        ),
    }
}

fn format_status(s: i32) -> &'static str {
    match ReadingStatus::try_from(s) {
        Ok(ReadingStatus::Planning) => "planning",
        Ok(ReadingStatus::Reading) => "reading",
        Ok(ReadingStatus::Completed) => "completed",
        Ok(ReadingStatus::OnHold) => "on_hold",
        Ok(ReadingStatus::Dropped) => "dropped",
        Ok(ReadingStatus::NotInterested) => "not_interested",
        _ => "unknown",
    }
}

#[tokio::main]
#[cfg_attr(coverage_nightly, coverage(off))]
async fn main() -> Result<()> {
    run().await
}

#[cfg_attr(coverage_nightly, coverage(off))]
async fn run() -> Result<()> {
    let cli = Cli::parse();
    let config = load_config();

    let server = config.server.unwrap_or(cli.server);
    let token = cli.token.or(config.token.clone());

    match cli.command {
        Command::Config {
            server: new_server,
            token: new_token,
        } => update_config(new_server, new_token),

        Command::Stats => fetch_stats(server).await,

        Command::Search { query, limit } => search_manga(server, query, limit).await,

        Command::Get { id } => get_manga(server, id).await,

        Command::List { page, per_page } => list_manga(server, page, per_page).await,

        Command::ListMine { status } => list_mine(server, token, status).await,

        Command::Track { manga_id, status } => track_manga(server, token, manga_id, status).await,

        Command::Progress { manga_id, progress } => {
            set_progress(server, token, manga_id, progress).await
        }

        Command::Rate { manga_id, score } => rate_manga(server, token, manga_id, score).await,

        Command::StatsMine => fetch_user_stats(server, token).await,
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
fn update_config(new_server: Option<String>, new_token: Option<String>) -> Result<()> {
    let mut config = load_config();
    if let Some(s) = new_server {
        config.server = Some(s);
    }
    if let Some(t) = new_token {
        config.token = Some(t);
    }
    save_config(&config)?;
    println!("Config saved to {}", config_path().display());
    if let Some(s) = &config.server {
        println!("  server: {}", s);
    }
    if config.token.is_some() {
        println!("  token: (set)");
    }
    Ok(())
}

#[cfg_attr(coverage_nightly, coverage(off))]
async fn fetch_stats(server: String) -> Result<()> {
    let client = GrpcWebClient::new(server, None);
    let resp: proto::Stats = client
        .call("manga.v1.MangaService", "GetStats", GetStatsRequest {})
        .await?;
    println!("Cleaned manga: {}", resp.cleaned_manga);
    println!("Raw manga: {}", resp.raw_manga);
    Ok(())
}

#[cfg_attr(coverage_nightly, coverage(off))]
async fn search_manga(server: String, query: String, limit: i32) -> Result<()> {
    let client = GrpcWebClient::new(server, None);
    let resp: proto::SearchMangaResponse = client
        .call(
            "manga.v1.MangaService",
            "SearchManga",
            SearchMangaRequest {
                query,
                limit,
                offset: 0,
            },
        )
        .await?;
    println!(
        "Found {} results ({}ms)",
        resp.estimated_total, resp.processing_time_ms
    );
    for manga in resp.items {
        let title = manga
            .title_english
            .or(manga.title_romaji)
            .unwrap_or_default();
        println!("{}: {}", manga.id, title);
    }
    Ok(())
}

#[cfg_attr(coverage_nightly, coverage(off))]
async fn get_manga(server: String, id: i64) -> Result<()> {
    let client = GrpcWebClient::new(server, None);
    let manga: Manga = client
        .call("manga.v1.MangaService", "GetManga", GetMangaRequest { id })
        .await?;
    println!("{}", serde_json::to_string_pretty(&manga_to_json(&manga))?);
    Ok(())
}

#[cfg_attr(coverage_nightly, coverage(off))]
async fn list_manga(server: String, page: i32, per_page: i32) -> Result<()> {
    let client = GrpcWebClient::new(server, None);
    let resp: proto::ListMangaResponse = client
        .call(
            "manga.v1.MangaService",
            "ListManga",
            ListMangaRequest {
                page,
                per_page,
                filter: None,
                sort_by: 0,
                sort_order: 0,
            },
        )
        .await?;
    if let Some(info) = resp.page_info {
        println!(
            "Page {} of {} ({} total)",
            info.page, info.total_pages, info.total
        );
    }
    for manga in resp.items {
        let title = manga
            .title_english
            .or(manga.title_romaji)
            .unwrap_or_default();
        println!("{}: {}", manga.id, title);
    }
    Ok(())
}

#[cfg_attr(coverage_nightly, coverage(off))]
async fn list_mine(server: String, token: Option<String>, status: Option<String>) -> Result<()> {
    let token = token.context("API token required. Run: sakuin config --token <token>")?;
    let client = GrpcWebClient::new(server, Some(token));
    let status_filter = status.map(|s| parse_status(&s)).transpose()?;
    let resp: proto::ListUserMangaResponse = client
        .call(
            "manga.v1.TrackerService",
            "ListUserManga",
            ListUserMangaRequest {
                status: status_filter.map(|s| s as i32),
            },
        )
        .await?;
    for entry in resp.items {
        let progress = entry
            .progress
            .map(|p| format!(" [{}]", p))
            .unwrap_or_default();
        println!(
            "{}: {} ({}){}",
            entry.manga_id,
            entry.title,
            format_status(entry.status),
            progress
        );
    }
    Ok(())
}

#[cfg_attr(coverage_nightly, coverage(off))]
async fn track_manga(
    server: String,
    token: Option<String>,
    manga_id: i64,
    status: String,
) -> Result<()> {
    let token = token.context("API token required. Run: sakuin config --token <token>")?;
    let client = GrpcWebClient::new(server, Some(token));
    let status = parse_status(&status)?;
    let resp: proto::UserReadingStatus = client
        .call(
            "manga.v1.TrackerService",
            "SetReadingStatus",
            SetReadingStatusRequest {
                manga_id,
                status: status as i32,
                progress: None,
            },
        )
        .await?;
    println!("Set {} to {}", manga_id, format_status(resp.status));
    Ok(())
}

#[cfg_attr(coverage_nightly, coverage(off))]
async fn set_progress(
    server: String,
    token: Option<String>,
    manga_id: i64,
    progress: String,
) -> Result<()> {
    let token = token.context("API token required. Run: sakuin config --token <token>")?;
    let client = GrpcWebClient::new(server, Some(token));
    let resp: proto::SetProgressResponse = client
        .call(
            "manga.v1.TrackerService",
            "SetProgress",
            SetProgressRequest { manga_id, progress },
        )
        .await?;
    println!("Set {} progress to {}", manga_id, resp.progress);
    Ok(())
}

#[cfg_attr(coverage_nightly, coverage(off))]
async fn rate_manga(
    server: String,
    token: Option<String>,
    manga_id: i64,
    score: i32,
) -> Result<()> {
    if !(1..=10).contains(&score) {
        anyhow::bail!("Score must be 1-10");
    }
    let token = token.context("API token required. Run: sakuin config --token <token>")?;
    let client = GrpcWebClient::new(server, Some(token));
    let resp: proto::Rating = client
        .call(
            "manga.v1.MangaService",
            "RateManga",
            RateMangaRequest { manga_id, score },
        )
        .await?;
    println!("Rated {} as {}/10", manga_id, resp.score);
    Ok(())
}

#[cfg_attr(coverage_nightly, coverage(off))]
async fn fetch_user_stats(server: String, token: Option<String>) -> Result<()> {
    let token = token.context("API token required. Run: sakuin config --token <token>")?;
    let client = GrpcWebClient::new(server, Some(token));
    let resp: proto::GetUserStatsResponse = client
        .call(
            "manga.v1.UserService",
            "GetUserStats",
            GetUserStatsRequest {
                token: String::new(),
            },
        )
        .await?;
    if let Some(stats) = resp.stats {
        println!("Planning: {}", stats.planning);
        println!("Reading: {}", stats.reading);
        println!("Completed: {}", stats.completed);
        println!("On hold: {}", stats.on_hold);
        println!("Dropped: {}", stats.dropped);
        println!("Not interested: {}", stats.not_interested);
        println!("Total: {}", stats.total);
        if let Some(avg) = stats.average_score {
            println!("Average score: {:.1}", avg);
        }
        println!("Ratings count: {}", stats.ratings_count);
    }
    Ok(())
}

fn manga_to_json(manga: &proto::Manga) -> serde_json::Value {
    serde_json::json!({
        "id": manga.id,
        "mangadex_id": manga.mangadex_id,
        "anilist_id": manga.anilist_id,
        "mal_id": manga.mal_id,
        "title_romaji": manga.title_romaji,
        "title_english": manga.title_english,
        "title_native": manga.title_native,
        "author": manga.author,
        "artist": manga.artist,
        "status": manga.status,
        "year": manga.year,
        "cover_url": manga.cover_url,
        "tags": manga.tags.iter().map(|t| serde_json::json!({
            "name": t.name,
            "group": t.group
        })).collect::<Vec<_>>(),
        "scores": manga.scores.as_ref().map(|s| serde_json::json!({
            "user_score": s.user_score,
            "user_count": s.user_count,
            "mangadex_score": s.mangadex_score,
            "anilist_score": s.anilist_score,
            "mal_score": s.mal_score,
        })),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn parse_status_accepts_documented_aliases() {
        assert_eq!(parse_status("planning").unwrap(), ReadingStatus::Planning);
        assert_eq!(parse_status("reading").unwrap(), ReadingStatus::Reading);
        assert_eq!(parse_status("completed").unwrap(), ReadingStatus::Completed);
        assert_eq!(parse_status("onhold").unwrap(), ReadingStatus::OnHold);
        assert_eq!(
            parse_status("not_interested").unwrap(),
            ReadingStatus::NotInterested
        );
        assert!(parse_status("later").is_err());
    }

    #[test]
    fn format_status_maps_known_and_unknown_codes() {
        assert_eq!(format_status(ReadingStatus::Planning as i32), "planning");
        assert_eq!(format_status(ReadingStatus::Reading as i32), "reading");
        assert_eq!(format_status(ReadingStatus::Completed as i32), "completed");
        assert_eq!(format_status(ReadingStatus::OnHold as i32), "on_hold");
        assert_eq!(format_status(ReadingStatus::Dropped as i32), "dropped");
        assert_eq!(
            format_status(ReadingStatus::NotInterested as i32),
            "not_interested"
        );
        assert_eq!(format_status(999), "unknown");
    }

    #[test]
    fn config_roundtrips_through_json() {
        let config = Config {
            server: Some("https://sakuin.test".to_string()),
            token: Some("token".to_string()),
        };

        let encoded = serde_json::to_string(&config).unwrap();
        let decoded: Config = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded.server.as_deref(), Some("https://sakuin.test"));
        assert_eq!(decoded.token.as_deref(), Some("token"));
    }

    #[test]
    fn manga_to_json_includes_scores_and_tags() {
        let manga = Manga {
            id: 42,
            title_english: Some("English".to_string()),
            tags: vec![proto::Tag {
                name: "Action".to_string(),
                group: "genre".to_string(),
            }],
            scores: Some(proto::MangaScores {
                user_score: Some(8.0),
                user_count: 3,
                mangadex_score: Some(7.0),
                anilist_score: Some(80),
                mal_score: Some(7.5),
            }),
            ..Manga::default()
        };

        let json = manga_to_json(&manga);

        assert_eq!(json["id"], 42);
        assert_eq!(json["title_english"], "English");
        assert_eq!(json["tags"][0]["name"], "Action");
        assert_eq!(json["scores"]["user_count"], 3);
    }
}
