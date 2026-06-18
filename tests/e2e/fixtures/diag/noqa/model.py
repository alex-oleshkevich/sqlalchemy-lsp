from __future__ import annotations

from sqlalchemy.orm import DeclarativeBase, Mapped, mapped_column


class Base(DeclarativeBase):
    pass


# This model has no __tablename__ — would normally trigger SQLA-W101.
# The # noqa suppresses the diagnostic on the class-name line.
class SuppressedModel(Base):  # noqa: SQLA-W101
    id: Mapped[int] = mapped_column(primary_key=True)


# This model also has no __tablename__ — bare # noqa suppresses all codes.
class AlsoSuppressed(Base):  # noqa
    id: Mapped[int] = mapped_column(primary_key=True)


# This model has no __tablename__ with no suppression → SQLA-W101 fires.
class UnsuppressedModel(Base):
    id: Mapped[int] = mapped_column(primary_key=True)
