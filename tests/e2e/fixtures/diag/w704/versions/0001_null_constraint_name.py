"""constraint operation with None as the constraint name → SQLA-W704"""
from alembic import op

revision = "w704rev1"
down_revision = None
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.drop_constraint(None, "users", type_="unique")


def downgrade() -> None:
    pass
