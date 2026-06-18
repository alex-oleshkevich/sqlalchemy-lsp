"""drop a table not in the model index"""
from alembic import op

revision = "h703rev1"
down_revision = None
branch_labels = None
depends_on = None


def upgrade() -> None:
    # "phantom_table" is not in any SQLAlchemy model and not created by this
    # migration → SQLA-H703
    op.add_column("phantom_table", "new_col")


def downgrade() -> None:
    op.drop_column("phantom_table", "new_col")
