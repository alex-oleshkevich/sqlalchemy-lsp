from sqlalchemy.orm import DeclarativeBase, Mapped, mapped_column


class Base(DeclarativeBase):
    pass


class User(Base):
    __tablename__ = "users"
    id: Mapped[int] = mapped_column(primary_key=True)


class UserAlias(Base):
    __tablename__ = "users"
    id: Mapped[int] = mapped_column(primary_key=True)
