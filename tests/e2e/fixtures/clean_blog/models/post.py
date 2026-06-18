from __future__ import annotations

from typing import List, Optional

from sqlalchemy import Column, ForeignKey, Integer, String, Table, Text
from sqlalchemy.orm import Mapped, mapped_column, relationship

from .base import Base

post_tags = Table(
    "post_tags",
    Base.metadata,
    Column("post_id", Integer, ForeignKey("posts.id"), primary_key=True),
    Column("tag_id", Integer, ForeignKey("tags.id"), primary_key=True),
)


class Post(Base):
    __tablename__ = "posts"

    id: Mapped[int] = mapped_column(Integer, primary_key=True)
    title: Mapped[str] = mapped_column(String(200))
    body: Mapped[Optional[str]] = mapped_column(Text, nullable=True)
    author_id: Mapped[int] = mapped_column(Integer, ForeignKey("users.id"))

    author: Mapped["User"] = relationship("User", back_populates="posts", lazy="joined")
    comments: Mapped[List["Comment"]] = relationship("Comment", back_populates="post")
    tags: Mapped[List["Tag"]] = relationship(
        "Tag", secondary=post_tags, back_populates="posts"
    )
