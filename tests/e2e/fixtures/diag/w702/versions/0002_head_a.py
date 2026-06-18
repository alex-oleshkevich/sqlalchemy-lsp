"""head a — parallel unmerged branch"""
from alembic import op

revision = "head0002a"
down_revision = "base0001"
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.add_column("users", "email")


def downgrade() -> None:
    op.drop_column("users", "email")
