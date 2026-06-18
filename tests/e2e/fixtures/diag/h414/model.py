from __future__ import annotations

from sqlalchemy import ForeignKey
from sqlalchemy.orm import DeclarativeBase, Mapped, mapped_column, relationship


class Base(DeclarativeBase):
    pass


class User(Base):
    __tablename__ = "users"
    id: Mapped[int] = mapped_column(primary_key=True)
    # List counterpart exists — this scalar uses lazy="select" (default) → SQLA-H414
    posts: Mapped[list["Post"]] = relationship("Post", back_populates="author", lazy="selectin")


class Post(Base):
    __tablename__ = "posts"
    id: Mapped[int] = mapped_column(primary_key=True)
    author_id: Mapped[int] = mapped_column(ForeignKey("users.id"))
    # lazy="select" (default) on scalar with list counterpart → SQLA-H414
    author: Mapped["User"] = relationship("User", back_populates="posts")
