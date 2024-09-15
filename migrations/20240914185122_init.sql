-- Create the users table with username as the primary key
CREATE TABLE IF NOT EXISTS users (
    username VARCHAR(255) PRIMARY KEY NOT NULL,
    password VARCHAR(255) NOT NULL
);

-- Add constraints to limit the length of username and password
ALTER TABLE users
ADD CONSTRAINT username_length CHECK (char_length(username) <= 255);

ALTER TABLE users
ADD CONSTRAINT password_length CHECK (char_length(password) <= 255);

-- Create the pastes table with username as a foreign key
CREATE TABLE IF NOT EXISTS pastes (
    id SERIAL PRIMARY KEY,
    creator_username VARCHAR(255) NOT NULL REFERENCES users(username) ON DELETE CASCADE,
    title VARCHAR(255) NOT NULL UNIQUE,
    content TEXT NOT NULL,
    views INTEGER DEFAULT 0,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Create a function to prevent updates to the title column
CREATE OR REPLACE FUNCTION prevent_title_update()
RETURNS TRIGGER AS $$
BEGIN
    IF OLD.title IS DISTINCT FROM NEW.title THEN
        RAISE EXCEPTION 'Title cannot be changed once set';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Create a trigger to use the above function
CREATE TRIGGER trigger_prevent_title_update
BEFORE UPDATE ON pastes
FOR EACH ROW
EXECUTE FUNCTION prevent_title_update();
