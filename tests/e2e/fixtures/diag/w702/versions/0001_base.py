"""base migration"""
from alembic import op

revision = "base0001"
down_revision = None
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.create_table("users")


def downgrade() -> None:
    op.drop_table("users")
