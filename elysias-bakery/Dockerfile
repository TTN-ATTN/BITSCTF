FROM oven/bun:latest

WORKDIR /app

COPY package.json bun.lock ./
RUN bun install --frozen-lockfile --production

COPY src/ src/
COPY public/ public/

RUN mkdir -p notes

COPY flag.txt /flag.txt

EXPOSE 3000

CMD ["bun", "run", "src/index.ts"]
