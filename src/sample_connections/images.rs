//! Images resolved by MusicBrainz identity, never by an ambiguous name search.

use anyhow::Result;
use serde_json::Value;

use super::{Priority, Provider, valid_id};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Target {
    Recording(String),
    Artist(String),
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
pub struct Picture {
    pub url: String,
    pub source: String,
    pub credit: String,
}

impl Provider {
    pub async fn picture(&self, target: &Target, priority: Priority) -> Result<Option<Picture>> {
        match target {
            Target::Recording(id) if valid_id(id) => {
                // Browse releases containing this exact recording, not a title match.
                let value = self
                    .get(
                        "release",
                        &[
                            ("recording", id.clone()),
                            ("inc", "release-groups".into()),
                            ("limit", "100".into()),
                            ("fmt", "json".into()),
                        ],
                        priority,
                    )
                    .await?;
                Ok(release_picture(&value))
            }
            Target::Artist(id) if valid_id(id) => {
                let value = self
                    .get(
                        &format!("artist/{id}"),
                        &[("inc", "url-rels".into()), ("fmt", "json".into())],
                        priority,
                    )
                    .await?;
                if value.get("id").and_then(Value::as_str) != Some(id) {
                    return Ok(None);
                }
                // Spotify's documented link-preview API needs no grant or browser.
                // Read the thumbnail only, never render the returned embed HTML.
                if let Some(artist) = spotify_artist(&value) {
                    let mut url = reqwest::Url::parse("https://open.spotify.com/oembed")?;
                    url.query_pairs_mut().append_pair("url", &artist);
                    if let Ok(preview) = self.get_url(url, priority).await
                        && let Some(picture) = spotify_picture(&preview, &artist)
                    {
                        return Ok(Some(picture));
                    }
                }
                // Only follow a verified Wikidata relationship for this artist.
                let Some(entity) = wikidata_entity(&value) else {
                    return Ok(None);
                };
                let url = format!("https://www.wikidata.org/wiki/Special:EntityData/{entity}.json");
                let value = self.get_url(url.parse()?, priority).await?;
                let Some(filename) = portrait_filename(&value, &entity) else {
                    return Ok(None);
                };
                let mut url = reqwest::Url::parse("https://commons.wikimedia.org/w/api.php")?;
                url.query_pairs_mut().extend_pairs([
                    ("action", "query"),
                    ("prop", "imageinfo"),
                    ("titles", &format!("File:{filename}")),
                    ("iiprop", "url|extmetadata"),
                    ("iiurlwidth", "256"),
                    ("format", "json"),
                    ("formatversion", "2"),
                ]);
                Ok(commons_picture(&self.get_url(url, priority).await?))
            }
            _ => Ok(None),
        }
    }
}

fn spotify_artist(value: &Value) -> Option<String> {
    value.get("relations")?.as_array()?.iter().find_map(|r| {
        if r.get("target-type").and_then(Value::as_str) != Some("url")
            || !matches!(
                r.get("type").and_then(Value::as_str),
                Some("free streaming" | "streaming")
            )
        {
            return None;
        }
        let url = reqwest::Url::parse(r.pointer("/url/resource")?.as_str()?).ok()?;
        let id = url.path().strip_prefix("/artist/")?;
        (url.host_str() == Some("open.spotify.com")
            && matches!(url.scheme(), "https" | "http")
            && id.len() == 22
            && id.bytes().all(|b| b.is_ascii_alphanumeric()))
        .then(|| format!("https://open.spotify.com/artist/{id}"))
    })
}

fn spotify_picture(value: &Value, artist: &str) -> Option<Picture> {
    let url = reqwest::Url::parse(value.get("thumbnail_url")?.as_str()?).ok()?;
    // Artist link previews sometimes use an album cover when no portrait exists.
    // Accept Spotify's artist-photo image family, not that album fallback.
    if !url.path().strip_prefix("/image/")?.starts_with("ab676161") {
        return None;
    }
    if url.scheme() != "https"
        || !matches!(
            url.host_str(),
            Some(
                "i.scdn.co"
                    | "image-cdn-ak.spotifycdn.com"
                    | "image-cdn-fa.spotifycdn.com"
                    | "image-cdn-ev.spotifycdn.com"
            )
        )
    {
        return None;
    }
    Some(Picture {
        url: url.into(),
        source: artist.into(),
        credit: "Spotify".into(),
    })
}

fn release_picture(value: &Value) -> Option<Picture> {
    let releases = value.get("releases")?.as_array()?;
    let release = releases
        .iter()
        .filter(|r| {
            r.get("id").and_then(Value::as_str).is_some_and(valid_id)
                && r.pointer("/cover-art-archive/front")
                    .and_then(Value::as_bool)
                    == Some(true)
        })
        .min_by_key(|r| {
            // Prefer an original official release over a later compilation.
            let compilation = r
                .pointer("/release-group/secondary-types")
                .and_then(Value::as_array)
                .is_some_and(|types| types.iter().any(|t| t == "Compilation"));
            (
                r.get("status").and_then(Value::as_str) != Some("Official"),
                compilation,
                r.get("date")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .unwrap_or("9999"),
                r.get("id").and_then(Value::as_str).unwrap_or_default(),
            )
        })?;
    let id = release.get("id")?.as_str()?;
    Some(Picture {
        url: format!("https://coverartarchive.org/release/{id}/front-250"),
        source: format!("https://musicbrainz.org/release/{id}/cover-art"),
        credit: "Cover Art Archive".into(),
    })
}

fn wikidata_entity(value: &Value) -> Option<String> {
    value.get("relations")?.as_array()?.iter().find_map(|r| {
        if r.get("type").and_then(Value::as_str) != Some("wikidata")
            || r.get("target-type").and_then(Value::as_str) != Some("url")
        {
            return None;
        }
        let url = reqwest::Url::parse(r.pointer("/url/resource")?.as_str()?).ok()?;
        if url.host_str() != Some("www.wikidata.org") || !matches!(url.scheme(), "http" | "https") {
            return None;
        }
        let id = url.path().strip_prefix("/wiki/")?;
        (id.starts_with('Q') && id.len() > 1 && id[1..].bytes().all(|b| b.is_ascii_digit()))
            .then(|| id.to_owned())
    })
}

fn portrait_filename<'a>(value: &'a Value, entity: &str) -> Option<&'a str> {
    value
        .get("entities")?
        .get(entity)?
        .pointer("/claims/P18")?
        .as_array()?
        .iter()
        .filter(|claim| claim.get("rank").and_then(Value::as_str) != Some("deprecated"))
        .filter_map(|claim| {
            let snak = claim.get("mainsnak")?;
            if snak.get("snaktype").and_then(Value::as_str) != Some("value")
                || snak.get("datatype").and_then(Value::as_str) != Some("commonsMedia")
            {
                return None;
            }
            let file = snak.pointer("/datavalue/value")?.as_str()?.trim();
            (!file.is_empty() && !file.contains('|')).then_some((claim, file))
        })
        .min_by_key(|(claim, _)| claim.get("rank").and_then(Value::as_str) != Some("preferred"))
        .map(|(_, filename)| filename)
}

fn commons_picture(value: &Value) -> Option<Picture> {
    let info = value
        .pointer("/query/pages")?
        .as_array()?
        .first()?
        .get("imageinfo")?
        .as_array()?
        .first()?;
    let url = info.get("thumburl").or_else(|| info.get("url"))?.as_str()?;
    let mut url = reqwest::Url::parse(url).ok()?;
    if url.scheme() != "https"
        || !matches!(
            url.host_str(),
            Some("upload.wikimedia.org" | "thumb.wikimedia.org")
        )
    {
        return None;
    }
    // The API can append campaign parameters. They are not needed to load a photo.
    url.set_query(None);
    url.set_fragment(None);
    let source = info.get("descriptionurl")?.as_str()?;
    let source_url = reqwest::Url::parse(source).ok()?;
    if source_url.scheme() != "https" || source_url.host_str() != Some("commons.wikimedia.org") {
        return None;
    }
    let metadata = info.get("extmetadata")?;
    let author = crate::util::strip_html(metadata.pointer("/Artist/value")?.as_str()?);
    let license = crate::util::strip_html(metadata.pointer("/LicenseShortName/value")?.as_str()?);
    if author.trim().is_empty() || license.trim().is_empty() {
        return None;
    }
    Some(Picture {
        url: url.into(),
        source: source.into(),
        credit: format!("{} · {} · Wikimedia Commons", author.trim(), license.trim()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn spotify_portraits_use_the_linked_artist_and_ignore_embed_html() {
        let artist = "https://open.spotify.com/artist/4Y7tXHSEejGu1vQ9bwDdXW";
        let linked = |url| json!({"relations":[{"type":"free streaming","target-type":"url","url":{"resource":url}}]});
        assert_eq!(spotify_artist(&linked(artist)).as_deref(), Some(artist));
        for bad in [
            "https://example.com/artist/4Y7tXHSEejGu1vQ9bwDdXW",
            "https://open.spotify.com/artist/invalid",
            "https://open.spotify.com/track/4Y7tXHSEejGu1vQ9bwDdXW",
        ] {
            assert!(spotify_artist(&linked(bad)).is_none());
        }
        let preview = json!({"thumbnail_url":"https://image-cdn-fa.spotifycdn.com/image/ab67616100005174c20ed3229eeced918966a009", "html":"<iframe>never rendered</iframe>"});
        let photo = spotify_picture(&preview, artist).unwrap();
        assert_eq!(photo.source, artist);
        assert_eq!(photo.credit, "Spotify");
        assert!(
            spotify_picture(
                &json!({"html":"<img src='https://example.com/photo'>"}),
                artist
            )
            .is_none()
        );
        assert!(
            spotify_picture(
                &json!({"thumbnail_url":"https://example.com/photo"}),
                artist
            )
            .is_none()
        );
        assert!(spotify_picture(&json!({"thumbnail_url":"https://i.scdn.co/image/ab67616d00001e02d959dca6f179a6387de63066"}), artist).is_none());
    }

    #[test]
    fn cover_uses_a_release_of_the_exact_recording_and_prefers_original_art() {
        let id = "5d617966-5ef7-46ea-96c7-1e43ee7b5d5b";
        let value = json!({"releases": [
            {"id": "../../elsewhere", "cover-art-archive": {"front": true}},
            {"id": "22c78c44-0ec6-4fb6-a96e-233eca4ed07a", "status":"Official", "date":"2006", "cover-art-archive":{"front":true}, "release-group":{"secondary-types":["Compilation"]}},
            {"id":id, "status":"Official", "date":"1972", "cover-art-archive":{"front":true}}
        ]});
        let picture = release_picture(&value).unwrap();
        assert!(picture.url.ends_with(&format!("{id}/front-250")));
        assert!(release_picture(&json!({"releases":[]})).is_none());
        assert!(
            release_picture(&json!({"releases":[{"id":id,"cover-art-archive":{"front":false}}]}))
                .is_none()
        );
    }

    #[test]
    fn portraits_follow_only_the_linked_entity_and_skip_deprecated_images() {
        let link = |url| json!({"relations":[{"type":"wikidata","target-type":"url","url":{"resource":url}}]});
        assert_eq!(
            wikidata_entity(&link("https://www.wikidata.org/wiki/Q272619")),
            Some("Q272619".into())
        );
        for bad in [
            "https://example.com/wiki/Q1",
            "https://www.wikidata.org/wiki/Q1/../../Q2",
            "https://www.wikidata.org/wiki/Q1x",
        ] {
            assert!(wikidata_entity(&link(bad)).is_none());
        }
        let image = |rank, file| json!({"rank":rank,"mainsnak":{"snaktype":"value","datatype":"commonsMedia","datavalue":{"value":file}}});
        let value = json!({"entities":{"Q1":{"claims":{"P18":[image("deprecated","Wrong.jpg"),image("normal","Old.jpg"),image("preferred","Portrait.jpg")]}}}});
        assert_eq!(portrait_filename(&value, "Q1"), Some("Portrait.jpg"));
        assert_eq!(portrait_filename(&value, "Q2"), None);
    }

    #[test]
    fn portrait_keeps_attribution_and_rejects_untrusted_hosts_or_missing_credits() {
        let mut value = json!({"query":{"pages":[{"imageinfo":[{
            "thumburl":"https://upload.wikimedia.org/wikipedia/commons/thumb/a/ab/Artist.jpg/256px-Artist.jpg?utm_source=test",
            "descriptionurl":"https://commons.wikimedia.org/wiki/File:Artist.jpg",
            "extmetadata":{"Artist":{"value":"<a href='url'>A &amp; B</a>"},"LicenseShortName":{"value":"CC BY 2.0"}}
        }]}]}});
        let picture = commons_picture(&value).unwrap();
        assert_eq!(picture.credit, "A & B · CC BY 2.0 · Wikimedia Commons");
        assert!(!picture.url.contains('?'));
        value["query"]["pages"][0]["imageinfo"][0]["thumburl"] =
            json!("https://example.com/face.jpg");
        assert!(commons_picture(&value).is_none());
        value["query"]["pages"][0]["imageinfo"][0]["extmetadata"] = Value::Null;
        assert!(commons_picture(&value).is_none());
    }
}
