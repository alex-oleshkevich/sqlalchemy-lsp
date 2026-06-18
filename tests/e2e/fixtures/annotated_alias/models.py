"""Models using Annotated type aliases (SQLAlchemy 2.0 style)."""
from __future__ import annotations

from typing import Annotated, Optional

from sqlalchemy import String
from sqlalchemy.orm import DeclarativeBase, Mapped, mapped_column

# ── Annotated type aliases ────────────────────────────────────────────────────

# Primary key alias — primary_key=True comes from the alias
intpk = Annotated[int, mapped_column(primary_key=True)]

# Nullable string alias
NullableStr = Annotated[Optional[str], mapped_column(String(200), nullable=True)]

# Non-nullable string alias
RequiredStr = Annotated[str, mapped_column(String(120), nullable=False)]


# ── Models ────────────────────────────────────────────────────────────────────


class Base(DeclarativeBase):
    pass


class Product(Base):
    __tablename__ = "products"

    # Primary key from alias — should be recognized as PK
    id: Mapped[intpk]

    # Required name from alias
    name: Mapped[RequiredStr]

    # Nullable description from alias
    description: Mapped[NullableStr]

    # Inline Annotated (no alias) — primary_key extracted from inline annotation
    sku: Mapped[Annotated[str, mapped_column(String(50), unique=True)]]
