# Analytics collector

## Prometheus job configuration

```
- job_name: "analytics-collector"
  kubernetes_sd_configs:
    - role: pod
  relabel_configs:
    - source_labels:
        - __meta_kubernetes_pod_name
      action: keep
      regex: "analytics-collector-.*"
    - source_labels:
        - __meta_kubernetes_pod_container_port_number
      regex: "8000"
      action: keep
```