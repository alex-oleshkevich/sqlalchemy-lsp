-- Drop into init.lua or after/plugin/sqlalchemy_lsp.lua
-- Requires Neovim 0.11+ with built-in vim.lsp.config.
vim.lsp.config('sqlalchemy_lsp', {
  cmd = { 'sqlalchemy-lsp', 'lsp', '--stdio' },
  filetypes = { 'python' },
  root_markers = { 'pyproject.toml', 'sqlalchemy-lsp.toml', 'alembic.ini', '.git' },
})
vim.lsp.enable('sqlalchemy_lsp')
