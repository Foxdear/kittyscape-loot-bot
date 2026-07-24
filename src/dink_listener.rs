use anyhow::Result;
use axum::Extension;
use axum::RequestExt;
use axum::extract::DefaultBodyLimit;
use axum::routing::post;
use serde_json::error::Category::Data;
use serenity::all::ChannelAction::Create;
use serenity::all::CreateEmbedAuthor;
use serenity::all::Http;
use serenity::all::{
    CreateAttachment, GatewayIntents, Interaction, Message, Ready
};
use sqlx::Sqlite;
use std::future::IntoFuture as _;
use serenity::async_trait;
use serenity::prelude::*;
use serenity::all::{CreateMessage, CreateEmbed};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;
use std::env;
use std::fs::File;
use std::sync::Arc;
use dotenvy::dotenv;
use axum::{
    body::{Body, Bytes},
    extract::{Request, Json, Query, Multipart},
    http::{header::CONTENT_TYPE, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
    Router,
    Form,
};
use serde_json::Value;
use tracing::{error, info, debug};
use crate::command_handler::{PriceManagerKey, CollectionLogManagerKey};
use crate::config::{Config, ConfigKey};
use crate::runescape_tracker::RunescapeTrackerKey;
use crate::DinkHandler;
use serde::Deserialize;
use std::collections::HashMap;
use std::borrow::Borrow;


// https://github.com/pajlads/DinkPlugin/blob/master/docs/json-examples.md


#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct DinkItem {
    id: i64,
    quantity: i64,
    price_each: i64,
    name: String,
    criteria: Vec<String>,
    rarity: Option<String>,
}
// All of these need to be Options because they may or may not be there
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct DinkExtra {
    // Loot category
    source: Option<String>,
    items: Option<Vec<DinkItem>>,
    category: Option<String>,
    kill_count: Option<i64>,
    // Pet category
    pet_name: Option<String>,
    milestone: Option<String>,
    duplicate: Option<bool>,
    // Clog category
    item_name: Option<String>,
    item_id: Option<i64>,
    price: Option<i64>,
    completed_entries: Option<i64>,
    total_entries: Option<i64>,
    dropper_name: Option<String>,
    dropper_type: Option<String>,
    dropper_kill_count: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct DinkPayload {
    // These are always sent
    content: Option<String>,
    r#type: String,
    player_name: String,
    account_type: String,
    seasonal_world: bool,
    dink_account_hash: String,
    group_iron_clan_name: Option<String>,
    extra: DinkExtra,
}

struct DinkFile {
    file_name: String,
    content: Bytes,
}

pub async fn dink_handler(dink_handler: Extension<DinkHandler>, mut multipart: Multipart) {

    // Initialize config
    let config = Config::from_env().unwrap();

    let mut dink_message: Message;
    // let mut data: DinkPayload;
    // let mut screenshot: CreateAttachment;
    let mut dink_file = DinkFile {
        file_name: "None".to_string(),
        content: Bytes::new(),
    };
    let mut payload_json = Bytes::new();
    

    while let Some(mut field) = multipart.next_field().await.unwrap() {
        let name = field.name().unwrap().to_string();
        match name.as_str() {
            "payload_json" => {
                debug!("Found payload");
                payload_json = field.bytes().await.unwrap();
                debug!("Payload length: {}", payload_json.len());
            }
            "file" => {
                debug!("Found screenshot");
                //info!("Field info: {:#?}", field);
                dink_file.file_name = field.file_name().unwrap().to_string();
                dink_file.content = field.bytes().await.unwrap();
            }
            _ => {error!("Error handling field: {:?}", name.as_str())}
            
        }
    }

    info!("Payload: {:#?}", payload_json);
    let data: DinkPayload = serde_json::from_slice(&payload_json).unwrap();
    //info!("Type: {:#?}", data.r#type);
    let screenshot = CreateAttachment::bytes(dink_file.content, dink_file.file_name);
    let author = CreateEmbedAuthor::new(data.player_name);
    if data.account_type.as_str() != "NORMAL" {
        author.icon_url("https://oldschool.runescape.wiki/images/".to_owned() + match data.account_type.as_str() {
            "IRONMAN" => {"Ironman_chat_badge.png"}
            "ULTIMATE_IRONMAN" => {"Ultimate_ironman_chat_badge.png"}
            "HARDCORE_IRONMAN" => {"Hardcore_ironman_chat_badge.png"}
            "GROUP_IRONMAN" => {"Group_ironman_chat_badge.png"}
            "HARDCORE_GROUP_IRONMAN" => {"Hardcore_group_ironman_chat_badge.png"}
            "UNRANKED_GROUP_IRONMAN" => {"Unranked_group_ironman_chat_badge.png"}
            _ => {"Cheese_detail.png"}
        });
    }
    let embed = CreateEmbed::new()
    .author(author)
    .image(format!("attachment://{}", screenshot.filename))
    .description("Duuuuuuude what the fuck is up")
    .field("Love", "```fix\nWomen```", true)
    .field("Sister", "```fix\nKisser```", true);
    let builder = CreateMessage::new().add_embed(embed);

    let _ = config.mod_channel_id.send_files(&dink_handler.http, [screenshot], builder).await;
    
    let allowed_useragents = ["RuneLite/", "HDOS/"];
}