from __future__ import annotations

from sqlalchemy import ForeignKey
from sqlalchemy.orm import DeclarativeBase, Mapped, mapped_column, relationship


class Base(DeclarativeBase):
    pass


class A(Base):
    __tablename__ = "a_table"
    id: Mapped[int] = mapped_column(primary_key=True)
    b_id: Mapped[int] = mapped_column(ForeignKey("b_table.id"))
    # No back_populates — forms cycle A→B→C→A → SQLA-H410
    b: Mapped["B"] = relationship("B", lazy="joined")


class B(Base):
    __tablename__ = "b_table"
    id: Mapped[int] = mapped_column(primary_key=True)
    c_id: Mapped[int] = mapped_column(ForeignKey("c_table.id"))
    c: Mapped["C"] = relationship("C", lazy="joined")


class C(Base):
    __tablename__ = "c_table"
    id: Mapped[int] = mapped_column(primary_key=True)
    a_id: Mapped[int] = mapped_column(ForeignKey("a_table.id"))
    a: Mapped["A"] = relationship("A", lazy="joined")
