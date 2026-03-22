FROM golang:1.26-alpine AS builder

WORKDIR /app
COPY go.mod go.sum ./
RUN go mod download
COPY . .
RUN CGO_ENABLED=0 GOOS=linux go build -ldflags="-s -w" -o /core ./cmd/core

FROM alpine:3.21
RUN apk --no-cache add ca-certificates
COPY --from=builder /core /core

# Read-only filesystem, drop all capabilities
RUN chmod +x /core
USER nobody:nobody

ENTRYPOINT ["/core"]
