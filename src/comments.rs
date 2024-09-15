use actix_identity::Identity;
use actix_web::{web, HttpResponse, Responder};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Deserialize)]
pub struct CommentForm {
    pub content: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Comment {
    pub id: i32,
    pub creator_username: String,
    pub content: String,
    pub paste_id: i32, // Add this field
    pub created_at: i64,
}

pub async fn create_comment(
    identity: Identity,
    form: web::Form<CommentForm>,
    pool: web::Data<PgPool>,
    path: web::Path<String>, // Changed to String to match paste title
) -> impl Responder {
    let paste_title = path.into_inner(); // Extract paste title from the Path wrapper

    // Retrieve the username from the session
    let creator_username = match identity.id() {
        Ok(username) => username,
        Err(_) => return HttpResponse::Unauthorized().body("User not logged in"),
    };

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs() as i64;

    // Ensure query matches the number of parameters
    let result = sqlx::query!(
        "INSERT INTO comments (creator_username, content, paste_id, created_at) VALUES ($1, $2, (SELECT id FROM pastes WHERE title = $3), $4)",
        creator_username,
        form.content,
        paste_title,
        timestamp
    )
    .execute(pool.get_ref())
    .await;

    match result {
        Ok(_) => {
            // Redirect to the paste page after comment creation
            let redirect_url = format!("/paste/{}", paste_title); // Adjust the URL as needed
            HttpResponse::Found()
                .append_header(("Location", redirect_url))
                .finish()
        }
        Err(sqlx::Error::Database(err)) if err.message().contains("foreign key constraint") => {
            HttpResponse::BadRequest().body("Invalid paste title")
        }
        Err(_) => HttpResponse::InternalServerError().body("Error adding comment"),
    }
}
