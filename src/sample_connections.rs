//! On-demand recording sample relationships from MusicBrainz, never audio.

use std::collections::{HashMap, VecDeque};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde_json::Value;
use tokio::sync::Mutex;
use tokio::time::Instant;

use crate::api::models::Track;
use crate::http::Http;
use crate::model::Loadable;

pub mod images;

const SAMPLES: &str = "9efd9ce9-e702-448b-8e76-641515e8fe62";
const ROOT: &str = "https://musicbrainz.org/ws/2/";
const MAX_BODY: usize = 2 * 1024 * 1024;
const CACHE_LIFETIME: Duration = Duration::from_secs(3600);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recording {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub disambiguation: String,
    pub duration_ms: Option<u32>,
}

impl Recording {
    pub fn source_url(&self) -> String {
        format!("https://musicbrainz.org/recording/{}", self.id)
    }

    pub fn spotify_query(&self) -> String {
        format!(
            "track:\"{}\" artist:\"{}\"",
            self.title.replace('"', " "),
            self.artist.replace('"', " ")
        )
    }
}

#[derive(Clone, Debug)]
pub enum Query {
    Song {
        title: String,
        artist: String,
        duration_ms: u32,
        isrc: Option<String>,
        cover: Option<String>,
    },
    Recording(Recording),
}

impl Query {
    pub fn from_track(track: &Track) -> Option<Self> {
        if track.is_local || !track.uri.starts_with("spotify:track:") {
            return None;
        }
        Some(Self::Song {
            title: track.name.clone(),
            artist: track.artist_names(),
            duration_ms: track.duration_ms,
            isrc: track.external_ids.isrc.clone(),
            cover: track
                .album
                .as_ref()
                .and_then(|album| crate::api::models::pick_image(&album.images, 160))
                .map(str::to_owned),
        })
    }

    pub fn title(&self) -> &str {
        match self {
            Self::Song { title, .. } => title,
            Self::Recording(recording) => &recording.title,
        }
    }

    pub fn artist(&self) -> &str {
        match self {
            Self::Song { artist, .. } => artist,
            Self::Recording(recording) => &recording.artist,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Connections {
    pub recording: Recording,
    pub contributors: Vec<Contributor>,
    pub samples_from: Vec<Recording>,
    pub sampled_by: Vec<Recording>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Contributor {
    pub id: String,
    pub name: String,
    pub roles: Vec<String>,
}

impl Contributor {
    pub fn source_url(&self) -> String {
        format!("https://musicbrainz.org/artist/{}", self.id)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Found {
    Connections(Connections),
    /// Search results require the listener's choice, never a guessed version.
    Candidates(Vec<Recording>),
}

#[derive(Clone, Debug)]
pub struct View {
    pub request: u64,
    pub query: Query,
    pub result: Loadable<Found>,
    /// Absent until visible, then pending or unavailable, or an attributed image.
    pub pictures: HashMap<images::Target, Option<images::Picture>>,
}

impl View {
    pub fn request_picture(&mut self, request: u64, target: &images::Target) -> bool {
        if request != self.request || self.pictures.contains_key(target) {
            return false;
        }
        let belongs = match (&self.result, target) {
            (Loadable::Loaded(Found::Connections(c)), images::Target::Artist(id)) => {
                c.contributors.iter().any(|artist| artist.id == *id)
            }
            (Loadable::Loaded(Found::Connections(c)), images::Target::Recording(id)) => {
                c.recording.id == *id
                    || c.samples_from
                        .iter()
                        .chain(&c.sampled_by)
                        .any(|r| r.id == *id)
            }
            (Loadable::Loaded(Found::Candidates(c)), images::Target::Recording(id)) => {
                c.iter().any(|r| r.id == *id)
            }
            _ => false,
        };
        if belongs {
            self.pictures.insert(target.clone(), None);
        }
        belongs
    }

    pub fn receive_picture(
        &mut self,
        request: u64,
        target: images::Target,
        picture: Option<images::Picture>,
    ) {
        if request == self.request
            && let Some(entry) = self.pictures.get_mut(&target)
        {
            *entry = picture;
        }
    }

    /// A completion cannot reopen a dismissed dialog or replace a newer song.
    pub fn receive(&mut self, request: u64, result: Result<Found, String>) {
        if self.request == request {
            self.result = Loadable::from_result(result);
        }
    }
}

struct Cached {
    key: String,
    at: Instant,
    value: Value,
}

struct State {
    next_request: Instant,
    cache: VecDeque<Cached>,
}

/// One provider per backend. The mutex also serializes requests and preserves
/// the cooldown when a dialog's task is cancelled or a new song is selected.
pub struct Provider {
    http: Http,
    state: Mutex<State>,
    root: String,
}

impl Provider {
    pub fn new(http: Http) -> Self {
        Self {
            http,
            state: Mutex::new(State {
                next_request: Instant::now(),
                cache: VecDeque::new(),
            }),
            root: ROOT.into(),
        }
    }

    async fn get(&self, id: &str, query: &[(&str, String)]) -> Result<Value> {
        let mut url = reqwest::Url::parse(&format!("{}{id}", self.root))?;
        url.query_pairs_mut().extend_pairs(query);
        self.get_url(url).await
    }

    async fn get_url(&self, url: reqwest::Url) -> Result<Value> {
        let key = url.to_string();
        let mut state = self.state.lock().await;
        state
            .cache
            .retain(|entry| entry.at.elapsed() < CACHE_LIFETIME);
        if let Some(cached) = state.cache.iter().find(|entry| entry.key == key) {
            return Ok(cached.value.clone());
        }
        if state.next_request.saturating_duration_since(Instant::now()) > Duration::from_secs(5) {
            bail!("MusicBrainz is busy. Please try again later.");
        }
        tokio::time::sleep_until(state.next_request).await;
        // Resolve the client after waiting, so proxy changes apply immediately.
        let client = self.http.client().map_err(anyhow::Error::msg)?;
        state.next_request = Instant::now() + Duration::from_millis(1100);
        let mut response = client
            .get(url)
            .header(
                reqwest::header::USER_AGENT,
                concat!(
                    "Spotifast/",
                    env!("CARGO_PKG_VERSION"),
                    " (https://github.com/crmne/spotifast)"
                ),
            )
            .timeout(Duration::from_secs(20))
            .send()
            .await
            .map_err(|error| error.without_url())?;
        if matches!(response.status().as_u16(), 429 | 503) {
            let cooldown = retry_after(response.headers());
            // An unrepresentable delay must not overflow or cause a retry storm.
            state.next_request = Instant::now()
                .checked_add(cooldown.max(Duration::from_millis(1100)))
                .unwrap_or_else(|| Instant::now() + Duration::from_secs(86400 * 365));
            bail!("MusicBrainz is busy. Please try again later.");
        }
        response
            .error_for_status_ref()
            .map_err(|error| error.without_url())?;
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            if body.len().saturating_add(chunk.len()) > MAX_BODY {
                bail!("MusicBrainz response is too large");
            }
            body.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&body).context("Invalid MusicBrainz response")?;
        if state.cache.len() == 32 {
            state.cache.pop_front();
        }
        state.cache.push_back(Cached {
            key,
            at: Instant::now(),
            value: value.clone(),
        });
        Ok(value)
    }

    pub async fn fetch(&self, query: &Query) -> Result<Found> {
        let recording = match query {
            Query::Recording(recording) => recording.clone(),
            Query::Song {
                title,
                artist,
                duration_ms,
                isrc,
                ..
            } => {
                let mut candidates = Vec::new();
                // ISRCs are helpful but sometimes reused for multiple edits.
                if let Some(isrc) = isrc.as_deref().filter(|s| valid_isrc(s)) {
                    candidates = self.search(&format!("isrc:{isrc}")).await?;
                }
                if candidates.is_empty() {
                    candidates = self
                        .search(&format!(
                            "recording:\"{}\" AND artist:\"{}\"",
                            escape_search(title),
                            escape_search(artist)
                        ))
                        .await?;
                }
                let exact: Vec<_> = candidates
                    .iter()
                    .filter(|recording| {
                        same_name(&recording.title, title)
                            && same_name(&recording.artist, artist)
                            && *duration_ms > 0
                            && recording
                                .duration_ms
                                .is_some_and(|length| length.abs_diff(*duration_ms) <= 3000)
                            && recording.disambiguation.is_empty()
                    })
                    .collect();
                if exact.len() != 1 {
                    return Ok(Found::Candidates(candidates));
                }
                exact[0].clone()
            }
        };
        if !valid_id(&recording.id) {
            bail!("Invalid MusicBrainz recording ID");
        }
        let value = self
            .get(
                &format!("recording/{}", recording.id),
                &[
                    (
                        "inc",
                        "recording-rels+artist-credits+isrcs+artist-rels+work-rels+work-level-rels"
                            .into(),
                    ),
                    ("fmt", "json".into()),
                ],
            )
            .await?;
        let connections = parse_connections(&value)?;
        if connections.recording.id != recording.id {
            bail!("MusicBrainz returned a different recording");
        }
        Ok(Found::Connections(connections))
    }

    async fn search(&self, query: &str) -> Result<Vec<Recording>> {
        let value = self
            .get(
                "recording/",
                &[
                    ("query", query.into()),
                    ("limit", "25".into()),
                    ("fmt", "json".into()),
                ],
            )
            .await?;
        let results = value
            .get("recordings")
            .and_then(Value::as_array)
            .context("Missing MusicBrainz recordings")?;
        let mut recordings = Vec::new();
        for value in results {
            if let Some(recording) = parse_recording(value)
                && !recordings.iter().any(|r: &Recording| r.id == recording.id)
            {
                recordings.push(recording);
            }
        }
        Ok(recordings)
    }
}

fn valid_id(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(index, byte)| {
            if [8, 13, 18, 23].contains(&index) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}

fn valid_isrc(isrc: &str) -> bool {
    isrc.len() == 12 && isrc.bytes().all(|byte| byte.is_ascii_alphanumeric())
}

fn escape_search(text: &str) -> String {
    text.chars()
        .flat_map(|ch| {
            if "+-!():^[]\"{}~*?|&\\/".contains(ch) {
                vec!['\\', ch]
            } else {
                vec![ch]
            }
        })
        .collect()
}

fn same_name(a: &str, b: &str) -> bool {
    fn normalized(text: &str) -> String {
        text.chars()
            .filter(|ch| ch.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect()
    }
    !a.is_empty() && normalized(a) == normalized(b)
}

fn parse_recording(value: &Value) -> Option<Recording> {
    let id = value.get("id")?.as_str()?.to_owned();
    let title = value.get("title")?.as_str()?.to_owned();
    if !valid_id(&id) || title.is_empty() {
        return None;
    }
    let artist = value
        .get("artist-credit")
        .and_then(Value::as_array)
        .map(|credits| {
            credits
                .iter()
                .map(|credit| {
                    let name = credit
                        .get("name")
                        .or_else(|| credit.get("artist")?.get("name"))
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let join = credit
                        .get("joinphrase")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    format!("{name}{join}")
                })
                .collect()
        })
        .unwrap_or_default();
    Some(Recording {
        id,
        title,
        artist,
        disambiguation: value
            .get("disambiguation")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .into(),
        duration_ms: value
            .get("length")
            .and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok()),
    })
}

pub fn parse_connections(value: &Value) -> Result<Connections> {
    let recording = parse_recording(value).context("Missing MusicBrainz recording")?;
    // Missing relations is a broken lookup, not evidence of no sample data.
    let relations = value
        .get("relations")
        .and_then(Value::as_array)
        .context("Missing MusicBrainz relationships")?;
    let mut connections = Connections {
        recording,
        contributors: contributors(value),
        samples_from: Vec::new(),
        sampled_by: Vec::new(),
    };
    for relation in relations {
        if relation.get("type-id").and_then(Value::as_str) != Some(SAMPLES)
            || relation.get("target-type").and_then(Value::as_str) != Some("recording")
        {
            continue;
        }
        let Some(recording) = relation.get("recording").and_then(parse_recording) else {
            continue;
        };
        let list = match relation.get("direction").and_then(Value::as_str) {
            Some("forward") => &mut connections.samples_from,
            Some("backward") => &mut connections.sampled_by,
            _ => continue,
        };
        if recording.id != connections.recording.id && !list.iter().any(|r| r.id == recording.id) {
            list.push(recording);
        }
    }
    for list in [&mut connections.samples_from, &mut connections.sampled_by] {
        list.sort_by_key(|r| {
            (
                r.artist.to_lowercase(),
                r.title.to_lowercase(),
                r.id.clone(),
            )
        });
    }
    Ok(connections)
}

fn contributors(value: &Value) -> Vec<Contributor> {
    fn add(credits: &mut Vec<Contributor>, artist: &Value, role: &str) {
        let Some(id) = artist
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| valid_id(id))
        else {
            return;
        };
        let Some(name) = artist
            .get("name")
            .and_then(Value::as_str)
            .filter(|name| !name.is_empty())
        else {
            return;
        };
        if let Some(credit) = credits.iter_mut().find(|credit| credit.id == id) {
            if !credit.roles.iter().any(|existing| existing == role) {
                credit.roles.push(role.into());
            }
        } else {
            credits.push(Contributor {
                id: id.into(),
                name: name.into(),
                roles: vec![role.into()],
            });
        }
    }
    fn artist_relations(credits: &mut Vec<Contributor>, relations: &[Value]) {
        for relation in relations {
            if relation.get("target-type").and_then(Value::as_str) != Some("artist") {
                continue;
            }
            let Some(role) = relation.get("type").and_then(Value::as_str) else {
                continue;
            };
            // Exclude non-credit relationships, such as dedication or naming.
            if ![
                "producer",
                "engineer",
                "mix",
                "mastering",
                "recording",
                "programming",
                "instrument",
                "vocal",
                "performer",
                "arranger",
                "orchestra",
                "chorus",
                "conductor",
                "DJ-mix",
                "editing",
                "composer",
                "lyricist",
                "writer",
            ]
            .contains(&role)
            {
                continue;
            }
            if let Some(artist) = relation.get("artist") {
                add(credits, artist, role);
            }
        }
    }
    let mut credits = Vec::new();
    if let Some(artists) = value.get("artist-credit").and_then(Value::as_array) {
        for credit in artists {
            if let Some(artist) = credit.get("artist") {
                add(&mut credits, artist, "main artist");
            }
        }
    }
    if let Some(relations) = value.get("relations").and_then(Value::as_array) {
        artist_relations(&mut credits, relations);
        for relation in relations {
            if relation.get("target-type").and_then(Value::as_str) == Some("work")
                && relation.get("type").and_then(Value::as_str) == Some("performance")
                && let Some(relations) = relation
                    .get("work")
                    .and_then(|work| work.get("relations"))
                    .and_then(Value::as_array)
            {
                artist_relations(&mut credits, relations);
            }
        }
    }
    credits
}

fn retry_after(headers: &reqwest::header::HeaderMap) -> Duration {
    let Some(value) = headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|h| h.to_str().ok())
    else {
        return Duration::from_secs(60);
    };
    if let Ok(seconds) = value.parse::<u64>() {
        return Duration::from_secs(seconds);
    }
    jiff::fmt::rfc2822::DateTimeParser::new()
        .parse_timestamp(value)
        .ok()
        .map(|date| {
            Duration::from_secs(
                date.as_second()
                    .saturating_sub(jiff::Timestamp::now().as_second())
                    .max(0) as u64,
            )
        })
        .unwrap_or(Duration::from_secs(60))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn rockafeller() -> Value {
        serde_json::from_str(include_str!("testdata/samples/rockafeller.json")).unwrap()
    }

    #[test]
    fn live_examples_preserve_both_sample_directions_and_exclude_remixes() {
        let rockafeller = parse_connections(&rockafeller()).unwrap();
        assert_eq!(rockafeller.samples_from.len(), 2);
        assert_eq!(rockafeller.sampled_by.len(), 4);
        assert!(
            rockafeller
                .samples_from
                .iter()
                .any(|r| r.title == "Sliced Tomatoes" && r.artist == "Just Brothers")
        );
        assert!(
            !rockafeller
                .samples_from
                .iter()
                .chain(&rockafeller.sampled_by)
                .any(|r| r.disambiguation.contains("remix"))
        );
        let tomatoes = parse_connections(
            &serde_json::from_str(include_str!("testdata/samples/tomatoes.json")).unwrap(),
        )
        .unwrap();
        assert!(tomatoes.samples_from.is_empty());
        assert_eq!(tomatoes.sampled_by.len(), 2);
        assert!(
            tomatoes
                .sampled_by
                .iter()
                .any(|r| r.id == rockafeller.recording.id)
        );
    }

    #[test]
    fn recording_and_work_credits_merge_roles_by_artist_identity() {
        let connections = parse_connections(&rockafeller()).unwrap();
        let fatboy = connections
            .contributors
            .iter()
            .find(|c| c.name == "Fatboy Slim")
            .unwrap();
        assert_eq!(fatboy.roles, ["main artist", "producer", "writer"]);
        let simon = connections
            .contributors
            .iter()
            .find(|c| c.name == "Simon Thornton")
            .unwrap();
        assert_eq!(simon.roles, ["engineer", "mix"]);
        assert!(
            connections
                .contributors
                .iter()
                .any(|c| c.name == "John Barry" && c.roles == ["writer"])
        );
        assert!(
            connections
                .contributors
                .iter()
                .any(|c| c.name == "Winifred Terry" && c.roles == ["writer"])
        );
        assert_eq!(connections.contributors.len(), 4);
    }

    #[test]
    fn malformed_or_duplicate_relations_cannot_create_false_links() {
        let mut value = rockafeller();
        let relations = value["relations"].as_array_mut().unwrap();
        let sample = relations
            .iter()
            .find(|r| r["type-id"] == SAMPLES)
            .unwrap()
            .clone();
        relations.push(sample.clone());
        let mut malformed = sample.clone();
        malformed["recording"]["id"] = json!("../../artist/private");
        relations.push(malformed);
        let mut wrong_direction = sample;
        wrong_direction["direction"] = json!("unknown");
        relations.push(wrong_direction);
        let connections = parse_connections(&value).unwrap();
        assert_eq!(connections.samples_from.len(), 2);
        assert_eq!(connections.sampled_by.len(), 4);
        value.as_object_mut().unwrap().remove("relations");
        assert!(parse_connections(&value).is_err());
    }

    #[test]
    fn closed_and_reopened_views_ignore_old_results() {
        let recording = parse_connections(&rockafeller()).unwrap().recording;
        let mut view = View {
            request: 2,
            query: Query::Recording(recording),
            result: Loadable::Loading,
            pictures: HashMap::new(),
        };
        view.receive(1, Ok(Found::Candidates(Vec::new())));
        assert!(view.result.is_loading());
        view.receive(2, Err("offline".into()));
        assert_eq!(view.result, Loadable::Failed("offline".into()));
    }

    #[test]
    fn pictures_are_requested_once_and_stale_or_unrequested_images_are_ignored() {
        let connections = parse_connections(&rockafeller()).unwrap();
        let target = images::Target::Recording(connections.samples_from[0].id.clone());
        let other = images::Target::Artist("661e20c8-3d82-4da2-94a2-97d9e11691c0".into());
        let mut view = View {
            request: 7,
            query: Query::Recording(connections.recording.clone()),
            result: Loadable::Loaded(Found::Connections(connections)),
            pictures: HashMap::new(),
        };
        assert!(!view.request_picture(6, &target));
        assert!(!view.request_picture(7, &images::Target::Recording("unrelated".into())));
        assert!(view.request_picture(7, &target));
        assert!(!view.request_picture(7, &target));
        let photo = images::Picture {
            url: "https://coverartarchive.org/image".into(),
            source: "https://musicbrainz.org/release/source".into(),
            credit: "Cover Art Archive".into(),
        };
        view.receive_picture(6, target.clone(), Some(photo.clone()));
        assert!(view.pictures[&target].is_none());
        view.receive_picture(7, other.clone(), Some(photo.clone()));
        assert!(!view.pictures.contains_key(&other));
        view.receive_picture(7, target.clone(), Some(photo.clone()));
        assert_eq!(view.pictures[&target], Some(photo));
        // A missing portrait is a completed lookup, not a request every frame.
        assert!(view.request_picture(7, &other));
        view.receive_picture(7, other.clone(), None);
        assert!(!view.request_picture(7, &other));
        assert!(matches!(
            view.result,
            Loadable::Loaded(Found::Connections(_))
        ));
    }

    #[tokio::test]
    async fn cover_lookup_uses_recording_identity_and_the_shared_cache_and_transport() {
        let id = "5d617966-5ef7-46ea-96c7-1e43ee7b5d5b";
        let (provider, task) = server(vec![ok(json!({"releases":[{
            "id":id, "status":"Official", "cover-art-archive":{"front":true}
        }]}))])
        .await;
        let target = images::Target::Recording("7da4bc62-6876-4f21-9f01-1c79967df855".into());
        let first = provider.picture(&target).await.unwrap().unwrap();
        assert!(first.url.ends_with(&format!("{id}/front-250")));
        assert_eq!(provider.picture(&target).await.unwrap(), Some(first));
        let requests = task.await.unwrap();
        assert!(
            requests[0]
                .0
                .starts_with("GET /release?recording=7da4bc62-6876-4f21-9f01-1c79967df855&")
        );
        assert!(!requests[0].0.to_lowercase().contains("authorization:"));
        let provider = Provider::new(Http::unavailable("must not connect".into()));
        assert!(
            provider
                .picture(&images::Target::Artist("../../other".into()))
                .await
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn source_ids_queries_and_local_files_are_checked() {
        assert!(!valid_id("https://example.com/recording"));
        assert_eq!(
            escape_search("Song (live) \"yes\""),
            "Song \\(live\\) \\\"yes\\\""
        );
        assert!(!valid_isrc("GBBMQ9800025 OR *"));
        assert!(valid_isrc("GBBMQ9800025"));
        assert!(
            Query::from_track(&Track {
                uri: "spotify:local:a:b:c:1".into(),
                ..Default::default()
            })
            .is_none()
        );
    }

    async fn server(
        answers: Vec<String>,
    ) -> (Provider, tokio::task::JoinHandle<Vec<(String, Instant)>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let mut requests = Vec::new();
            for answer in answers {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut bytes = [0u8; 8192];
                let count = socket.read(&mut bytes).await.unwrap();
                requests.push((
                    String::from_utf8_lossy(&bytes[..count]).into_owned(),
                    Instant::now(),
                ));
                socket.write_all(answer.as_bytes()).await.unwrap();
            }
            requests
        });
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let mut provider = Provider::new(Http::new(client));
        provider.root = format!("http://{address}/");
        (provider, task)
    }

    fn ok(body: Value) -> String {
        let body = body.to_string();
        format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    fn song() -> Query {
        Query::Song {
            title: "The Rockafeller Skank".into(),
            artist: "Fatboy Slim".into(),
            duration_ms: 413_880,
            isrc: Some("GBBMQ9800025".into()),
            cover: None,
        }
    }

    #[tokio::test]
    async fn isrc_versions_use_duration_and_cached_results_make_no_repeat_requests() {
        let album = rockafeller();
        let mut edit = album.clone();
        edit["id"] = json!("5a0afb66-b6a5-42d4-9acc-4e4276d78763");
        edit["length"] = json!(238_640);
        let (provider, task) = server(vec![
            ok(json!({"recordings": [edit, album.clone()]})),
            ok(album),
        ])
        .await;
        let result = provider.fetch(&song()).await.unwrap();
        assert!(
            matches!(&result, Found::Connections(c) if c.recording.id == "4e5f16e7-330e-4459-aa30-71a7aabac392")
        );
        assert_eq!(provider.fetch(&song()).await.unwrap(), result);
        let requests = task.await.unwrap();
        assert!(
            requests[1]
                .0
                .starts_with("GET /recording/4e5f16e7-330e-4459-aa30-71a7aabac392?")
        );
        assert!(requests[1].0.contains("work-level-rels"));
        assert!(requests[0].0.contains("Spotifast/"));
        assert!(!requests[0].0.to_lowercase().contains("authorization:"));
        assert!(requests[1].1.duration_since(requests[0].1) >= Duration::from_secs(1));
    }

    #[tokio::test]
    async fn ambiguous_matches_and_missing_lengths_require_a_choice() {
        for missing_length in [false, true] {
            let mut first = rockafeller();
            first["disambiguation"] = json!("");
            let mut second = first.clone();
            second["id"] = json!("5a0afb66-b6a5-42d4-9acc-4e4276d78763");
            if missing_length {
                first["length"] = Value::Null;
                second["length"] = Value::Null;
            }
            let (provider, task) = server(vec![ok(json!({"recordings": [first, second]}))]).await;
            assert!(
                matches!(provider.fetch(&song()).await.unwrap(), Found::Candidates(c) if c.len() == 2)
            );
            task.await.unwrap();
        }
    }

    #[tokio::test]
    async fn busy_service_preserves_cooldown_without_retrying_or_caching_failure() {
        let (provider, task) = server(vec!["HTTP/1.1 503 Busy\r\nRetry-After: 120\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into()]).await;
        assert!(provider.fetch(&song()).await.is_err());
        task.await.unwrap();
        let state = provider.state.lock().await;
        assert!(state.next_request.duration_since(Instant::now()) > Duration::from_secs(115));
        assert!(state.cache.is_empty());
    }

    #[tokio::test]
    async fn oversized_and_invalid_json_are_errors_not_empty_results() {
        for body in ["a".repeat(MAX_BODY + 1), "not json".into()] {
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let (provider, task) = server(vec![response]).await;
            assert!(provider.fetch(&song()).await.is_err());
            task.await.unwrap();
        }
    }

    #[test]
    fn retry_after_accepts_dates_and_extreme_seconds_without_overflow() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::RETRY_AFTER,
            "Thu, 01 Jan 2099 00:00:00 GMT".parse().unwrap(),
        );
        assert!(retry_after(&headers) > Duration::from_secs(86400));
        headers.insert(
            reqwest::header::RETRY_AFTER,
            u64::MAX.to_string().parse().unwrap(),
        );
        assert_eq!(retry_after(&headers), Duration::from_secs(u64::MAX));
    }
}
