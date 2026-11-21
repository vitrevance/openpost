# openpost

A self-hosted publishing service: write posts in Markdown or HTML and publish
them instantly — no signup, no email. Posts are public at short URLs and can
be edited later by logging in with a token.

> **Save your token.** It is the only way to access and edit your posts again.
> There is no recovery mechanism if you lose it.

## Setup with Docker Compose

Requires Docker with the Compose plugin.

```sh
docker compose up -d --build
```

openpost is now running at <http://localhost:3000> (a PostgreSQL database is
started alongside it, with a persistent volume; migrations run automatically
at startup).

Configuration:

- Use a different host port: `OPENPOST_PORT=8080 docker compose up -d`
- Set your public URL: `BASE_URL=https://openpost.example.com docker compose up -d`
- Stop everything: `docker compose down` (add `-v` to also drop the database data)

## Setup with Docker

```sh
# Build the image
docker build -t openpost:latest .

# Start PostgreSQL (if you don't have one already)
docker run -d --name openpost-pg -p 5432:5432 \
  -e POSTGRES_USER=openpost -e POSTGRES_PASSWORD=openpost \
  -e POSTGRES_DB=openpost postgres:17

# Run openpost (migrations are applied automatically at startup)
docker run -d --name openpost -p 3000:3000 \
  -e DATABASE_URL=postgres://openpost:openpost@host.docker.internal:5432/openpost?sslmode=disable \
  openpost:latest
```

On Linux, replace `host.docker.internal` with the host's IP or use
`--network host`.

## Command line arguments

The `openpost` binary takes no command-line arguments. It is configured
entirely through environment variables:

| Variable       | Default                      | Meaning                               |
|----------------|------------------------------|---------------------------------------|
| `DATABASE_URL` | *(required)*                 | PostgreSQL connection string          |
| `BIND_ADDR`    | `0.0.0.0:3000`               | Address the HTTP server listens on    |
| `BASE_URL`     | `http://localhost:3000`      | Public base URL used in API responses |
| `TEMPLATE_DIR` | `/app/templates` (in Docker) | Directory with HTML templates         |
| `STATIC_DIR`   | `/app/static` (in Docker)    | Directory with static assets          |
| `RUST_LOG`     | `info`                       | Log filter                            |

## License

MIT — see [LICENSE](LICENSE).
