
### Running prometheus and rabbitmq

Need to mount the chart directory in the ubuntu VM on macos. If on Ubuntu skip step 2.

1. `microk8s helm install prometheus prometheus-community/kube-prometheus-stack -f prometheus.yaml`
2. `microk8s helm install rabbitmq oci://registry-1.docker.io/bitnamicharts/rabbitmq -f ./prometheus-rabbitmq.yaml`
