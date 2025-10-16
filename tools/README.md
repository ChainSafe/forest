### Unit tests

- run `go test -v .`

### Validate Forest metrics

```bash
wget http://localhost:6116/metrics -O metrics.txt
go run . metrics.txt
```
