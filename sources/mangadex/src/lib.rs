use ito_rs::models::manga::{
    Chapter, ContentRating, Manga, PageResult, Status, Viewer,
};
use ito_rs::models::{
    FilterItem, HomeComponent, HomeComponentValue, HomeLayout, Listing, Page, PageContent,
};
use ito_rs::net::Request;
use ito_rs::provider::MangaProvider;
use ito_rs::{Error, Result, export_manga_plugin};
use serde_json::Value;

const API: &str = "https://api.mangadex.org";
const TRANSLATED_LANGUAGE: &str = "en";

struct MangaDex;

fn get_json(url: &str) -> Result<Value> {
    let mut request = Request::get(url);
    request.header("Accept", "application/json");
    request.header("User-Agent", "Ito-MangaDex/0.1");
    request.rate_limit(5);

    let response = request.send()?;
    if !(200..300).contains(&response.status) {
        return Err(Error::Net(format!(
            "MangaDex returned HTTP {} for {}",
            response.status, url
        )));
    }

    serde_json::from_slice::<Value>(&response.body)
        .map_err(|error| Error::Host(format!("Unable to decode MangaDex JSON: {error}")))
}

fn localized_text(value: &Value, field: &str) -> Option<String> {
    let entries = value.get(field)?.as_object()?;
    for language in ["en", "ja-ro", "ja"] {
        if let Some(text) = entries.get(language).and_then(Value::as_str) {
            if !text.is_empty() {
                return Some(text.to_string());
            }
        }
    }

    entries
        .values()
        .find_map(|entry| entry.as_str().map(ToString::to_string))
}

fn relationship_name(item: &Value, relationship_type: &str) -> Vec<String> {
    item.get("relationships")
        .and_then(Value::as_array)
        .map(|relationships| {
            relationships
                .iter()
                .filter(|relationship| {
                    relationship.get("type").and_then(Value::as_str) == Some(relationship_type)
                })
                .filter_map(|relationship| {
                    relationship
                        .get("attributes")
                        .and_then(|attributes| attributes.get("name"))
                        .and_then(Value::as_str)
                        .map(ToString::to_string)
                })
                .collect()
        })
        .unwrap_or_default()
}

fn cover_filename(item: &Value) -> Option<String> {
    item.get("relationships")?
        .as_array()?
        .iter()
        .find(|relationship| {
            relationship.get("type").and_then(Value::as_str) == Some("cover_art")
        })
        .and_then(|relationship| relationship.get("attributes"))
        .and_then(|attributes| attributes.get("fileName"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn map_status(status: Option<&str>) -> Status {
    match status {
        Some("ongoing") => Status::Ongoing,
        Some("completed") => Status::Completed,
        Some("cancelled") => Status::Cancelled,
        Some("hiatus") => Status::Hiatus,
        _ => Status::Unknown,
    }
}

fn map_content_rating(rating: Option<&str>) -> (ContentRating, i32) {
    match rating {
        Some("suggestive") => (ContentRating::Suggestive, 1),
        Some("erotica") | Some("pornographic") => (ContentRating::Nsfw, 2),
        _ => (ContentRating::Safe, 0),
    }
}

fn manga_from_item(item: &Value) -> Manga {
    let id = item
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let attributes = item.get("attributes").unwrap_or(&Value::Null);

    let title = localized_text(attributes, "title")
        .or_else(|| {
            attributes
                .get("altTitles")?
                .as_array()?
                .iter()
                .find_map(|title| title.as_object()?.values().find_map(Value::as_str))
                .map(ToString::to_string)
        })
        .unwrap_or_else(|| "Untitled".to_string());

    let tags = attributes.get("tags").and_then(Value::as_array).map(|tags| {
        tags.iter()
            .filter_map(|tag| localized_text(tag.get("attributes")?, "name"))
            .collect::<Vec<_>>()
    });
    let cover = cover_filename(item)
        .map(|filename| format!("https://uploads.mangadex.org/covers/{id}/{filename}.512.jpg"));
    let authors = relationship_name(item, "author");
    let artists = relationship_name(item, "artist");
    let (content_rating, nsfw) =
        map_content_rating(attributes.get("contentRating").and_then(Value::as_str));

    Manga {
        key: id.clone(),
        title,
        authors: (!authors.is_empty()).then_some(authors),
        artist: artists.into_iter().next(),
        description: localized_text(attributes, "description"),
        tags,
        cover,
        url: Some(format!("https://mangadex.org/title/{id}")),
        status: map_status(attributes.get("status").and_then(Value::as_str)),
        content_rating,
        nsfw,
        viewer: Viewer::Rtl,
        chapters: None,
    }
}

fn manga_list_url(page: i32, query: Option<&str>) -> String {
    let page = page.max(1);
    let limit = 20;
    let offset = (page - 1) * limit;
    let mut url = format!(
        "{API}/manga?limit={limit}&offset={offset}&includes[]=cover_art&includes[]=author&includes[]=artist&originalLanguage[]=ja&availableTranslatedLanguage[]={TRANSLATED_LANGUAGE}&contentRating[]=safe&contentRating[]=suggestive&order[latestUploadedChapter]=desc"
    );

    if let Some(query) = query {
        if !query.trim().is_empty() {
            url.push_str("&title=");
            url.push_str(&urlencoding::encode(query));
        }
    }

    url
}

fn fetch_manga_page(page: i32, query: Option<&str>) -> Result<PageResult> {
    let payload = get_json(&manga_list_url(page, query))?;
    let entries = payload
        .get("data")
        .and_then(Value::as_array)
        .map(|items| items.iter().map(manga_from_item).collect::<Vec<_>>())
        .unwrap_or_default();
    let total = payload
        .get("total")
        .and_then(Value::as_i64)
        .unwrap_or(entries.len() as i64);
    let offset = payload
        .get("offset")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let limit = payload
        .get("limit")
        .and_then(Value::as_i64)
        .unwrap_or(20);

    Ok(PageResult {
        entries,
        has_next_page: offset + limit < total,
    })
}

fn fetch_details(id: &str) -> Result<Manga> {
    let url = format!(
        "{API}/manga/{id}?includes[]=cover_art&includes[]=author&includes[]=artist"
    );
    let payload = get_json(&url)?;
    let item = payload
        .get("data")
        .ok_or_else(|| Error::Host("MangaDex response did not contain manga data".into()))?;

    Ok(manga_from_item(item))
}

fn fetch_chapters(id: &str) -> Result<Vec<Chapter>> {
    let mut chapters = Vec::new();
    let mut offset = 0_i64;

    loop {
        let url = format!(
            "{API}/manga/{id}/feed?translatedLanguage[]={TRANSLATED_LANGUAGE}&includes[]=scanlation_group&order[volume]=desc&order[chapter]=desc&limit=100&offset={offset}"
        );
        let payload = get_json(&url)?;
        let items = payload
            .get("data")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        for item in &items {
            let chapter_id = item
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let attributes = item.get("attributes").unwrap_or(&Value::Null);
            let scanlator = relationship_name(item, "scanlation_group")
                .into_iter()
                .next();

            chapters.push(Chapter {
                key: chapter_id.clone(),
                title: attributes
                    .get("title")
                    .and_then(Value::as_str)
                    .filter(|title| !title.is_empty())
                    .map(ToString::to_string),
                volume: attributes
                    .get("volume")
                    .and_then(Value::as_str)
                    .and_then(|volume| volume.parse::<f32>().ok()),
                chapter: attributes
                    .get("chapter")
                    .and_then(Value::as_str)
                    .and_then(|chapter| chapter.parse::<f32>().ok()),
                date_updated: None,
                scanlator,
                url: Some(format!("https://mangadex.org/chapter/{chapter_id}")),
                lang: attributes
                    .get("translatedLanguage")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                paywalled: Some(false),
            });
        }

        let total = payload
            .get("total")
            .and_then(Value::as_i64)
            .unwrap_or(items.len() as i64);
        offset += items.len() as i64;
        if items.is_empty() || offset >= total {
            break;
        }
    }

    Ok(chapters)
}

impl MangaProvider for MangaDex {
    fn get_home() -> Result<HomeLayout> {
        let latest = fetch_manga_page(1, None)?.entries;
        Ok(HomeLayout {
            components: vec![HomeComponent {
                title: Some("Latest Updates".to_string()),
                subtitle: Some("Japanese-origin manga on MangaDex".to_string()),
                value: HomeComponentValue::MangaList(
                    false,
                    Some(2),
                    latest,
                    Some(Listing {
                        id: "latest".to_string(),
                        name: "Latest Updates".to_string(),
                        kind: 0,
                    }),
                ),
            }],
        })
    }

    fn get_manga_list(_listing: Listing, page: i32) -> Result<PageResult> {
        fetch_manga_page(page, None)
    }

    fn get_search_manga_list(
        query: &str,
        page: i32,
        _filters: Vec<FilterItem>,
    ) -> Result<PageResult> {
        fetch_manga_page(page, Some(query))
    }

    fn get_manga_update(
        manga: Manga,
        needs_details: bool,
        needs_chapters: bool,
    ) -> Result<Manga> {
        let mut updated = if needs_details {
            fetch_details(&manga.key)?
        } else {
            manga
        };

        if needs_chapters {
            updated.chapters = Some(fetch_chapters(&updated.key)?);
        }

        Ok(updated)
    }

    fn get_page_list(_manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
        let payload = get_json(&format!("{API}/at-home/server/{}", chapter.key))?;
        let base_url = payload
            .get("baseUrl")
            .and_then(Value::as_str)
            .ok_or_else(|| Error::Host("MangaDex response did not contain baseUrl".into()))?;
        let chapter_payload = payload
            .get("chapter")
            .ok_or_else(|| Error::Host("MangaDex response did not contain chapter data".into()))?;
        let hash = chapter_payload
            .get("hash")
            .and_then(Value::as_str)
            .ok_or_else(|| Error::Host("MangaDex response did not contain a chapter hash".into()))?;
        let files = chapter_payload
            .get("data")
            .and_then(Value::as_array)
            .ok_or_else(|| Error::Host("MangaDex response did not contain page files".into()))?;

        Ok(files
            .iter()
            .enumerate()
            .filter_map(|(index, file)| {
                let filename = file.as_str()?;
                Some(Page {
                    index: index as i32,
                    content: PageContent::Url(format!("{base_url}/data/{hash}/{filename}")),
                    has_description: false,
                    description: None,
                    headers: None,
                })
            })
            .collect())
    }
}

export_manga_plugin!(MangaDex);

