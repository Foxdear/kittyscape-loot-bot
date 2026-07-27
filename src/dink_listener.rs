use axum::Extension;
use axum::http::HeaderMap;
use serenity::all::ArgumentConvert;
use serenity::all::CreateEmbedAuthor;
use serenity::all::CreateEmbedFooter;
use serenity::all::{
    CreateAttachment, Timestamp
};
use std::future::IntoFuture as _;
use serenity::async_trait;
use serenity::prelude::*;
use serenity::all::{CreateMessage, CreateEmbed, Member};
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
};
use tracing::{error, info, debug};
use crate::command_handler::{PriceManagerKey, CollectionLogManagerKey};
use crate::config::Config;
use crate::runescape_tracker::RunescapeTrackerKey;
use crate::DinkHandler;
use serde::Deserialize;
use crate::logger;
use crate::command_handler::utils;

// https://github.com/pajlads/DinkPlugin/blob/master/docs/json-examples.md


#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct DinkItem {
    id: i32,
    quantity: i64,
    price_each: i64,
    name: String,
    criteria: Vec<String>,
    rarity: Option<f64>,
}
// All of these need to be Options because they may or may not be there
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct DinkExtra {
    // Loot category
    source: Option<String>,
    items: Option<Vec<DinkItem>>,
    category: Option<String>,
    kill_count: Option<i32>,
    // Pet category
    pet_name: Option<String>,
    milestone: Option<String>,
    duplicate: Option<bool>,
    // Clog category
    item_name: Option<String>,
    item_id: Option<i32>,
    price: Option<i64>,
    completed_entries: Option<i32>,
    total_entries: Option<i32>,
    dropper_name: Option<String>,
    dropper_type: Option<String>,
    dropper_kill_count: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct DinkDiscord {
    id: String,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct DinkPayload {
    // These are always sent
    content: Option<String>,
    #[serde(alias = "type")]
    notif_type: String,
    player_name: String,
    account_type: String,
    seasonal_world: bool,
    dink_account_hash: String,
    discord_user: Option<DinkDiscord>,
    extra: DinkExtra,
}

struct DinkFile {
    file_name: String,
    content: Bytes,
}

pub async fn dink_handler(dink_handler: Extension<DinkHandler>, headers: HeaderMap, mut multipart: Multipart) {

    let agent = headers.get("user-agent").unwrap().clone();
    let agent_str = agent.to_str().unwrap();
    // This wouldn't scale well but these are the only two we need to worry about
    // Check user-agent to minimize bad requests
    if agent_str.starts_with("RuneLite/") || agent_str.starts_with("HDOS/") {
        // Initialize config
        let config = Config::from_env().unwrap();
        // let mut data: DinkPayload;
        // let mut screenshot: CreateAttachment;
        let mut dink_file = DinkFile {
            file_name: "None".to_string(),
            content: Bytes::new(),
        };
        let mut payload_json = Bytes::new();

        while let Some(field) = multipart.next_field().await.unwrap() {
            let name = field.name().unwrap();
            match name {
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
                _ => {error!("Error handling field: {:?}", name)}
                
            }
        }

        //Test data that can be used to force specific values
        let test_data = serde_json::json!({
            "content": "Text message as set by the user",
            "extra": {
                "items": [
                {
                    "id": 22327,
                    "quantity": 1,
                    "priceEach": 9041814,
                    "name": "Justiciar chestguard",
                    "criteria": ["VALUE"],
                    "rarity": 0.1666666666666667
                }
                ],
                "source": "Tombs of Amascut",
                "party": ["%USERNAME%", "another RSN", "yet another RSN"],
                "category": "EVENT",
                "killCount": 60,
                "rarestProbability": 0.001,
                "npcId": null
            },
            "type": "LOOT",
            "playerName": "multiboob",
            "accountType": "IRONMAN",
            "seasonalWorld": false,
            "dinkAccountHash": "abcdefghijklmnopqrstuvwxyz1234abcdefghijklmnopqrstuvwxyz",
            "embeds": []
            });
        debug!("Payload: {:#?}", payload_json);
        let data: DinkPayload = serde_json::from_slice(&payload_json).unwrap();
        //let data: DinkPayload = serde_json::from_value(test_data).unwrap();
        //info!("Type: {:#?}", data.r#type);
        let screenshot = CreateAttachment::bytes(dink_file.content, dink_file.file_name);
        let mut author = CreateEmbedAuthor::new(data.player_name.clone());
        if data.account_type.as_str() != "NORMAL" { //Mains don't have rights
            let icon = match data.account_type.as_str() {
                "IRONMAN" => "Ironman_chat_badge.png",
                "ULTIMATE_IRONMAN" => "Ultimate_ironman_chat_badge.png",
                "HARDCORE_IRONMAN" => "Hardcore_ironman_chat_badge.png",
                "GROUP_IRONMAN" => "Group_ironman_chat_badge.png",
                "HARDCORE_GROUP_IRONMAN" => "Hardcore_group_ironman_chat_badge.png",
                "UNRANKED_GROUP_IRONMAN" => "Unranked_group_ironman_chat_badge.png",
                _ => "Cheese_detail.png",
            };
            author = author.icon_url(format!("https://oldschool.runescape.wiki/images/{icon}"));
        }
        let mut embed = CreateEmbed::new()
        .author(author)
        .image(format!("attachment://{}", screenshot.filename))
        .timestamp(Timestamp::now());
        let discord_member = identify_user(data.clone(), dink_handler.db.clone(), dink_handler.ctx.clone()).await;
        if discord_member.is_some() && !data.seasonal_world {
            //If we can't find the user an account belongs to, they probably shouldn't be getting posted
            //Also don't care about leagues (for now)
            let member = discord_member.unwrap();
            let footer = CreateEmbedFooter::new(member.display_name())
            .icon_url(member.face());
            embed = embed.footer(footer);

            let discord_id = member.user.id.to_string();

            let user_record = sqlx::query!(
                "SELECT * FROM users 
                WHERE discord_id = ?",
                discord_id
            )
            .fetch_one(&dink_handler.db)
            .await
            .ok()
            .unwrap();

            let mut sendable = false;

            match data.notif_type.as_str() {
                "COLLECTION" => {
                    sendable = true;
                    debug!("Received collection log");
                    let id = data.extra.item_id.clone().unwrap();
                    let item = sqlx::query!("SELECT * FROM v_item_data WHERE item_id = ?", id)
                    .fetch_one(&dink_handler.db)
                    .await
                    .ok();
                    //Initiate
                    let description: String;
                    if item.is_some() {
                        let item = item.unwrap();
                        let item_name = item.preferred_name.unwrap();
                        //Do they have this item recorded already?
                        if let Ok(Some(_)) = sqlx::query!(
                            "SELECT id FROM collection_log_entries 
                            WHERE discord_id = ? AND item_name = ?",
                            discord_id,
                            item_name
                        )
                        .fetch_optional(&dink_handler.db)
                        .await
                        {
                            debug!("User {} already has collection log entry for {}", discord_id, item_name);
                            description = format!("Got a collection log item:\n**{}**!\n\n...But it was already recorded!", search_link(item_name.clone()));

                            let _ = logger::log_action(
                                &dink_handler.ctx,
                                &discord_id,
                                "DINK CLOG",
                                &format!("{} received collection log item they already had: {}", data.player_name, item_name)
                            ).await;
                        } else {
                            let points = dink_clog(&dink_handler, id, item_name.clone(), discord_id.clone()).await;
                            description = format!("Got a new collection log item:\n**{}**!", search_link(item_name.clone()));
                            //Now that we know for sure the item is valid we can build the embed
                            
                            embed = embed.field("Global Clog Rate", format_value(format!("{}%", item.percentage.unwrap())), true)
                            .field("Collection Log", format_value(format!("{}/{}", data.extra.completed_entries.unwrap(), data.extra.total_entries.unwrap())), true)
                            .field("", "", false);

                            if data.extra.dropper_name.is_some() || data.extra.dropper_kill_count.is_some() {
                                embed = field_if_exists(embed, data.extra.dropper_name, "Source");
                                embed = field_if_exists(embed, data.extra.dropper_kill_count, "Count");
                                embed = embed.field("", "", false);
                            }

                            embed = embed.field("Points Added", format_value(format!("+{}", points)), true)
                            .field("Points Total", format_value(user_record.points.to_string()), true);
                            
                            let _ = logger::log_action(
                                &dink_handler.ctx,
                                &discord_id,
                                "DINK CLOG",
                                &format!("{} received collection log item: {} (+{} points)", data.player_name, item_name, points)
                            ).await;
                        }
                        
                    }
                    else {
                        let item_name = data.extra.item_name.unwrap();
                        //If we don't have a record for it, it's probably new
                        //We don't have data so we kinda just have to abandon ship
                        description = format!("Got a new collection log item:\n**{}**!\n\nBut, ummm... I don't know what that is yet... sorry...", search_link(item_name.clone()));
                        //We can still add the record but no points will be added
                        dink_clog(&dink_handler, id, item_name.clone(), discord_id.clone()).await;
                        let _ = logger::log_action(
                                &dink_handler.ctx,
                                &discord_id,
                                "DINK CLOG",
                                &format!("{} received collection log item, but it was unknown (zero points given): {}", data.player_name, item_name)
                            ).await;
                    }
                    embed = embed.thumbnail(format!("https://static.runelite.net/cache/item/icon/{id}.png"))
                        .description(description);
                }
                "LOOT" => {
                    debug!("Received drop");
                    //Currently, drops worth 100k+ gp = 1 point per 100k
                    //We first need to check if the drop is even worth that much
                    //Part of the reason for this update is because RDT uncut sapphires (for example) are technically ~1/170
                    //Technically *rare* when some valuable drops are 1/128, but when people had their drop plugins set improperly we'd get notified for basically everything
                    //Dink does not have rarity set for every drop (according to docs), so we'll just say if it's high enough value it's fine
                    let mut valuable: Option<DinkItem> = None;
                    let items = data.extra.items.unwrap();
                    let mut best: i64 = 0;
                    for (_i, item) in items.iter().enumerate() {
                        let value = item.quantity * item.price_each;
                        //Annoyingly even if an item is in the denylist, it's still sent if we get other drop data, just with DENYLIST criteria
                        if value > 0 && value > best && !item.criteria.contains(&"DENYLIST".to_string()) { //Change this back to 100_000
                            valuable = Some(item.clone());
                            best = value;
                        }
                    }
                    if valuable.is_some() {
                        //Now that we know it's valuable, we're okay to send
                        sendable = true;
                        let item = valuable.unwrap();
                        let points = best / 100_000;
                        let source = data.extra.source.unwrap();
                        let description = if item.quantity > 1 {
                            format!("Got {}x {} from {}!", item.quantity, search_link(item.name.clone()), search_link(source))
                        }
                        else {
                            format!("Got {} from {}!", search_link(item.name.clone()), search_link(source))
                        };
                        if item.rarity.is_some() {
                            let rarity_val = item.rarity.unwrap();
                            let denom = (1.0 / rarity_val).round();
                            let rarity = rarity_val * 100.0;
                            embed = embed.field("Rarity (approx)", format!("```glsl\n# 1/{} ({:.2}%)```", denom, rarity), true);
                        }
                        embed = embed.description(description)
                        .field("GE Price", format_value(utils::format_gp(best)), true)
                        .field("", "", false)
                        .field("Points Added", format_value(format!("+{}", points)), true)
                        .field("Points Total", format_value(user_record.points.to_string()), true)
                        .thumbnail(format!("https://static.runelite.net/cache/item/icon/{}.png", item.id));
                        dink_drop(&dink_handler, item.id, item.name.clone(), best, discord_id.clone()).await;
                        
                        // Log the auto-added drop to the bot log channel
                        let _ = crate::logger::log_action(
                            &dink_handler.ctx,
                            &discord_id,
                            "DINK DROP",
                            &format!("{} received {}x {} worth {} GP", data.player_name, item.quantity, item.name, best)
                        ).await;
                    }
                    else {
                        // Log the auto-added drop to the bot log channel
                        let _ = crate::logger::log_action(
                            &dink_handler.ctx,
                            &discord_id,
                            "DINK DROP (REJECTED)",
                            &format!("{} tried to log some hot garbage", data.player_name)
                        ).await;
                    }
                }
                "PET" => {
                    sendable = true;
                    debug!("Received pet");
                    //We don't process this for points (yet) so it's pretty simple, relatively
                    let description = match data.extra.duplicate.unwrap() {
                        true => {
                            "You have a funny feeling like you would've been followed..."
                        }
                        false => {
                            "You have a funny feeling like you're being followed..."
                        }
                    };
                    embed = embed.description(description);
                    if data.extra.pet_name.is_some() { //Is the name set?
                        let pet_name = data.extra.pet_name.unwrap();
                        embed = embed.field("Pet", format_value(pet_name.clone()), true);
                        //Check db for item_id (Dink doesn't give this for pets)
                        let pet_row = sqlx::query!(
                            "SELECT item_id, item_name FROM collection_log_items WHERE item_name = ?",
                            pet_name
                        )
                        .fetch_one(&dink_handler.db)
                        .await;
                        if pet_row.is_ok() {
                            let pet_id = pet_row.unwrap().item_id;
                            embed = embed.thumbnail(format!("https://static.runelite.net/cache/item/icon/{pet_id}.png"));
                        }
                        if data.extra.milestone.is_some() { //Do we have a milestone?
                            embed = embed.field("Milestone", format_value(data.extra.milestone.unwrap()), true);
                        }
                    }
                }
                _ => {
                    debug!("Received type we don't handle");
                    let _ = logger::log_action(
                        &dink_handler.ctx,
                        &member.user.id.to_string(),
                        "UNSUPPORTED EVENT",
                        &format!("{} tried to use Dink type {}", data.player_name, data.notif_type)
                    ).await;
                }
            }
            if sendable {
                let builder = CreateMessage::new().add_embed(embed);

                let _ = config.runelite_channel_id.unwrap().send_files(&dink_handler.ctx.http, [screenshot], builder).await;
            }
            
        }
        else {
            let _ = logger::log_generic(
                &dink_handler.ctx,
                &format!("UNKNOWN USER: Someone I couldn't find in the discord server tried to do something: RSN {} sent something with Dink of type {}", data.player_name, data.notif_type)
            ).await;
        }
    }  
}
//This function is so if you want to change the formatting on everything, you can ("fix" makes the text blue)
fn format_value (value: String) -> String {
    format!("```fix\n{value}```")
}
async fn identify_user (data: DinkPayload, db: SqlitePool, ctx: Context) -> Option<Member> {
    // Initialize config
    let config = Config::from_env().ok()?;

    let mut member: Option<Member> = None; 

    //Okay who are we dealing with here
    //Check username and hash (if we can't find one we'll find the other)
    //The only way this goes wrong (returns two rows) is if someone changes their name and a second person takes the old name
    //That'd be fucked up. I'm not accounting for that
    let username = data.player_name;
    let hash = data.dink_account_hash;
    let user = sqlx::query!("SELECT * FROM runescape_accounts 
    WHERE runescape_name = ? OR dink_hash = ?",
    username, hash)
    .fetch_one(&db)
    .await;
    //Did we find anyone
    if user.is_ok() {
        //Ok cool. Does all our info match?
        let user = user.unwrap();
        if !(user.dink_hash == Some(hash.clone()) && user.runescape_name == username) {
            //Either we don't have a hash yet, or the username got changed
            let _ = sqlx::query!("UPDATE runescape_accounts
            SET runescape_name = ?, dink_hash = ?
            WHERE id = ?",
            username, hash, user.id)
            .execute(&db).await;
        }
        let member_data = Member::convert(ctx, Some(config.guild_id), config.runelite_channel_id, &user.discord_id.as_str()).await;
        if member_data.is_ok() {
            member = member_data.ok();
        }
    }
    //Well who the fuck is this then
    //Do they have discord data in the notification?
    else if data.discord_user.is_some() {
        //Is this person actually in our server?
        let discord_id = data.discord_user.unwrap().id;
        let member_data = Member::convert(ctx.clone(), Some(config.guild_id), config.runelite_channel_id, discord_id.as_str()).await;
        if member_data.is_ok() {
            //If they're in the server, and using the plugin, we assume they WANT to be tracked
            //We'll just help them automagically
            member = member_data.ok();

            // Link RS name to Discord ID
            let _ = sqlx::query!(
                "INSERT INTO runescape_accounts (discord_id, runescape_name, dink_hash) 
                VALUES (?, ?, ?)
                ON CONFLICT(discord_id, runescape_name) DO NOTHING",
                discord_id,
                username,
                hash
            )
            .execute(&db)
            .await;

            // Log the action
            let _ = logger::log_action(
                &ctx,
                &discord_id,
                "AUTOLINKED RSNAME",
                &format!("{}", username)
            ).await;
        }
    }
    member
}
async fn dink_clog(handler: &Extension<DinkHandler>, item_id: i32, name: String, discord_id: String) -> i64 {
    let points = handler.collection_log_manager.calculate_points_dink(item_id).await;
    let points = if points.is_some() { points.unwrap() } else { 0 };

    // Record the collection log entry
    let _ = sqlx::query!(
        "INSERT INTO collection_log_entries (discord_id, item_name, points, item_id) VALUES (?, ?, ?, ?)",
        discord_id,
        name,
        points,
        item_id
    )
    .execute(&handler.db)
    .await;

    if points > 0 {
        // Update user points
        let _ = sqlx::query!(
            "UPDATE users 
            SET points = points + ?
            WHERE discord_id = ?",
            points,
            discord_id
        )
        .execute(&handler.db)
        .await;
    }

    points
}
async fn dink_drop(handler: &Extension<DinkHandler>, item_id: i32, name: String, value: i64, discord_id: String) {

    // Record the collection log entry
    let _ = sqlx::query!(
        "INSERT INTO drops (discord_id, item_name, value, item_id) VALUES (?, ?, ?, ?)",
        discord_id,
        name,
        value,
        item_id
    )
    .execute(&handler.db)
    .await;

    let points = value / 100_000;

    // Update user points
    let _ = sqlx::query!(
        "UPDATE users 
        SET points = points + ?
        WHERE discord_id = ?",
        points,
        discord_id
    )
    .execute(&handler.db)
    .await;

}
fn field_if_exists(embed: CreateEmbed, value: Option<String>, name: &str) -> CreateEmbed {
    if value.is_some() { embed.field(name, value.unwrap(), true) } else { embed }
}
fn search_link(name: String) -> String {
    let link = format!("https://oldschool.runescape.wiki/w/Special:Search?search={}", name.clone().replace(" ", "%20"));
    format!("[{}]({})", name, link)
}