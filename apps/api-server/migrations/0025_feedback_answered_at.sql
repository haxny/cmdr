-- When David marked this feedback message answered, by hand from the dashboard (`PUT /admin/feedback/:id/answered`).
-- Replies go out from his own mailbox, which the Worker never sees, so this is the only way the dashboard's
-- "Awaiting reply" count learns a reply happened. NULL means it still needs one (when it has a reply-to email).
ALTER TABLE feedback ADD COLUMN answered_at TEXT;
