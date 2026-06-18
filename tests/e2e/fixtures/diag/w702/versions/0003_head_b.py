"""head b — second unmerged branch → SQLA-W702"""
from alembic import op

revision = "head0003b"
down_revision = "base0001"
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.add_column("users", "name")


def downgrade() -> None:
    op.drop_column("users", "name")
