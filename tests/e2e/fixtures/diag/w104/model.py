from sqlalchemy.orm import DeclarativeBase, Mapped, mapped_column


class Base(DeclarativeBase):
    pass


class Tag(Base):
    __tablename__ = "tags"
    name: Mapped[str] = mapped_column()
