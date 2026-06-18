from __future__ import annotations

from sqlalchemy.orm import DeclarativeBase, Mapped, mapped_column


class Base(DeclarativeBase):
    pass


class Post(Base):
    __tablename__ = "posts"
    id: Mapped[int] = mapped_column(primary_key=True)
    # Mutable list as default is shared across rows → SQLA-W203
    tags: Mapped[str] = mapped_column(default=[])
