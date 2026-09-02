use sea_orm_migration::prelude::*;

/// Chat — channels, membership and messages.
///
/// Built rather than bought. The tradeoff was argued the other way (Delightree
/// itself runs CometChat) and the call went to building; what follows is shaped
/// so that owning it stays cheap: three tables, no bespoke delivery
/// infrastructure, and fan-out over the Postgres `LISTEN`/`NOTIFY` the task
/// router already proves works across this fleet.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
            -- A channel belongs to exactly one org. There is no cross-org
            -- channel and there must never be one: the org is the tenancy
            -- boundary, and a conversation spanning two of them has no owner
            -- that could answer a deletion request.
            CREATE TABLE chat_channels (
                id          UUID PRIMARY KEY,
                org_id      UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
                -- 'channel' is named and joinable; 'dm' is a private thread
                -- between a fixed set of members and has no name of its own.
                kind        TEXT NOT NULL DEFAULT 'channel'
                            CHECK (kind IN ('channel', 'dm')),
                name        TEXT,
                topic       TEXT,
                created_by  UUID REFERENCES users(id) ON DELETE SET NULL,
                created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
                archived_at TIMESTAMPTZ,
                -- A named channel must actually have a name; a DM must not,
                -- because its title is derived from its members and a stored
                -- one would go stale the moment somebody leaves.
                CONSTRAINT chat_channels_name_matches_kind
                    CHECK ((kind = 'channel' AND name IS NOT NULL)
                        OR (kind = 'dm' AND name IS NULL))
            );
            CREATE INDEX chat_channels_by_org ON chat_channels (org_id, archived_at);

            -- Membership is the read gate. Every channel query filters on this
            -- rather than asking the authz model, because a user's channel set
            -- is unbounded — loading it into PrincipalFacts on every request
            -- would put an unbounded read on the hot path to answer a question
            -- a WHERE clause answers for free.
            CREATE TABLE chat_channel_members (
                channel_id   UUID NOT NULL REFERENCES chat_channels(id) ON DELETE CASCADE,
                user_id      UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                joined_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
                -- Unread is derived from this, not counted into a column: a
                -- stored counter has to be updated by every writer and drifts
                -- the first time one of them fails.
                last_read_at TIMESTAMPTZ,
                -- Notification preference, per member per channel.
                muted        BOOLEAN NOT NULL DEFAULT false,
                PRIMARY KEY (channel_id, user_id)
            );
            CREATE INDEX chat_channel_members_by_user ON chat_channel_members (user_id);

            CREATE TABLE chat_messages (
                id         UUID PRIMARY KEY,
                channel_id UUID NOT NULL REFERENCES chat_channels(id) ON DELETE CASCADE,
                author_id  UUID REFERENCES users(id) ON DELETE SET NULL,
                body       TEXT NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                edited_at  TIMESTAMPTZ,
                -- Soft delete. A hard DELETE leaves a hole in a conversation
                -- somebody was reading; a tombstone renders as "message
                -- deleted" and keeps the thread legible.
                deleted_at TIMESTAMPTZ
            );

            -- The paging index. `(channel_id, created_at DESC, id DESC)`
            -- rather than `(channel_id, created_at)`: a channel is always read
            -- newest-first, and the id tiebreak is what makes keyset paging
            -- total when two messages share a timestamp — without it a page
            -- boundary can drop or repeat a message.
            CREATE INDEX chat_messages_page
                ON chat_messages (channel_id, created_at DESC, id DESC);
        "#,
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
            DROP TABLE IF EXISTS chat_messages CASCADE;
            DROP TABLE IF EXISTS chat_channel_members CASCADE;
            DROP TABLE IF EXISTS chat_channels CASCADE;
        "#,
            )
            .await?;
        Ok(())
    }
}
