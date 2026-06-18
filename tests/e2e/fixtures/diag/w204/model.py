from __future__ import annotations

from sqlalchemy import text
from sqlalchemy.orm import DeclarativeBase, Mapped, mapped_column


class Base(DeclarativeBase):
    pass


class Item(Base):
    __tablename__ = "items"
    id: Mapped[int] = mapped_column(primary_key=True)
    # Both default and server_default set → SQLA-W204
    status: Mapped[str] = mapped_column(default="active", server_default=text("'active'"))
