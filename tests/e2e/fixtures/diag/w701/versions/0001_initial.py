"""initial migration"""
from alembic import op

revision = "aabbccdd"
down_revision = "00000000"  # does not exist → SQLA-W701
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.create_table("users")


def downgrade() -> None:
    op.drop_table("users")
