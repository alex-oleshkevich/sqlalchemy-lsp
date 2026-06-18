from __future__ import annotations

from typing import List, Optional

from sqlalchemy import ForeignKey, Integer, Text
from sqlalchemy.orm import Mapped, mapped_column, relationship

from .base import Base


class Comment(Base):
    __tablename__ = "comments"

    id: Mapped[int] = mapped_column(Integer, primary_key=True)
    body: Mapped[str] = mapped_column(Text)
    post_id: Mapped[int] = mapped_column(Integer, ForeignKey("posts.id"))
    parent_id: Mapped[Optional[int]] = mapped_column(
        Integer, ForeignKey("comments.id"), nullable=True
    )

    post: Mapped["Post"] = relationship("Post", back_populates="comments")
    parent: Mapped[Optional["Comment"]] = relationship(
        "Comment", remote_side="Comment.id", back_populates="replies"
    )
    replies: Mapped[List["Comment"]] = relationship(
        "Comment", back_populates="parent"
    )
