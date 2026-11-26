# hazardd on k3s (personal deployment)

Manifests for the private cluster (`gps.teintinu.com`). This branch exists only for deployment and is
never proposed upstream.

## Image

Built for `linux/amd64` by `.github/workflows/hazardd-image.yml` and published to
`ghcr.io/teintinu/hazardd:v1`.

## First install

```bash
kubectl create namespace gps
kubectl -n gps create secret generic hazardd-env \
  --from-literal=POSTGRES_PASSWORD="$(openssl rand -hex 16)" \
  --from-literal=TOKEN_PEPPER="$(openssl rand -hex 32)" \
  --from-literal=DATABASE_URL="postgres://hazard:<same-password>@postgres.gps.svc.cluster.local:5432/hazard"
kubectl apply -f deploy/k8s/postgres.yaml
kubectl -n gps rollout status statefulset/postgres
kubectl apply -f deploy/k8s/hazardd.yaml
kubectl apply -f deploy/k8s/ingress.yaml
```

DNS: `gps.teintinu.com` A records point at both node IPs. TLS is issued by cert-manager
(`letsencrypt-prod`).

## Upgrade

```bash
kubectl -n gps set image deploy/hazardd hazardd=ghcr.io/teintinu/hazardd:v1
kubectl -n gps rollout status deploy/hazardd
```
