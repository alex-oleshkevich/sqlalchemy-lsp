from __future__ import annotations

from typing import Optional

from sqlalchemy.orm import DeclarativeBase, Mapped, mapped_column


class Base(DeclarativeBase):
    pass


class User(Base):
    __tablename__ = "users"
    id: Mapped[int] = mapped_column(primary_key=True)
    # Optional annotation but nullable=False contradicts it → SQLA-H202
    nickname: Mapped[Optional[str]] = mapped_column(nullable=False)
