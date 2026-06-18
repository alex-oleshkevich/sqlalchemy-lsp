"""add posts and supporting tables

Revision ID: b2c3d4e5
Revises: a1b2c3d4
Create Date: 2026-01-02 00:00:00.000000
"""
from alembic import op
import sqlalchemy as sa

revision = "b2c3d4e5"
down_revision = "a1b2c3d4"
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.create_table(
        "tags",
        sa.Column("id", sa.Integer, primary_key=True),
        sa.Column("name", sa.String(50), nullable=False, unique=True),
    )
    op.create_table(
        "posts",
        sa.Column("id", sa.Integer, primary_key=True),
        sa.Column("title", sa.String(200), nullable=False),
        sa.Column("body", sa.Text, nullable=True),
        sa.Column("author_id", sa.Integer, sa.ForeignKey("users.id"), nullable=False),
    )
    op.create_table(
        "comments",
        sa.Column("id", sa.Integer, primary_key=True),
        sa.Column("body", sa.Text, nullable=False),
        sa.Column("post_id", sa.Integer, sa.ForeignKey("posts.id"), nullable=False),
        sa.Column("parent_id", sa.Integer, sa.ForeignKey("comments.id"), nullable=True),
    )
    op.create_table(
        "profiles",
        sa.Column("id", sa.Integer, primary_key=True),
        sa.Column("user_id", sa.Integer, sa.ForeignKey("users.id"), nullable=False),
        sa.Column("bio", sa.Text, nullable=True),
    )
    op.create_table(
        "post_tags",
        sa.Column("post_id", sa.Integer, sa.ForeignKey("posts.id"), primary_key=True),
        sa.Column("tag_id", sa.Integer, sa.ForeignKey("tags.id"), primary_key=True),
    )


def downgrade() -> None:
    op.drop_table("post_tags")
    op.drop_table("profiles")
    op.drop_table("comments")
    op.drop_table("posts")
    op.drop_table("tags")
