# hitscanner
- 什么是hitscanner
    - 一个基于ansible, k8s和airflow的网络拓扑测量系统
    - 基于当前成熟解决方案和最佳实践，系统更加健壮、灵活
- 为什么使用ansible:
    - 通用：内置任务模组,兼容多种发行版本
    - 独立：无其他依赖项，纯ssh
    - 便捷：一键bootstrap k8s集群
- 为什么使用airflow：
    - 解耦：基于configuration as code原则, 用DAG定义任务，便于实现复杂数据管道
    - 健壮：当个节点任务失败不会影响整个任务
- 为什么使用k8s：
    - 可靠: k8s久经检验，整合业界最佳实践,管理测量集群更健壮、灵活，
    - 兼容：airflow有KubernetesExecutor

## 0. 背景知识
- kubernetes load balancer 和 ingress
    - ingress是应用层负载均衡
    - ingress是反向代理（如nginx）
    - load balancer在k8s特指传输层负载均衡
- Rust programming language
    - Rust's references are fundamentally smart pointers [ref](https://stackoverflow.com/questions/64167637/is-the-concept-of-reference-different-in-c-and-rust)

## 1. 先决条件
- master node with at least 2 IP Addresses
- slave nodes
- domain name points to the master node
- https certificate for the domain name
- nodes running ubuntu
- ansible
```bash
apt-get install -y ansible
```
- key pair
```bash
ssh-keygen
# k3s.id_rsa, k3s.pub
```

## 2. 私仓搭建
- 为什么不搭在集群内？
    - 因为需要从集群内访问，搭在集群内会访问不了（k8s）
    - 其实可以做到从集群内访问但是太复杂，例如 [trow](https://github.com/ContainerSolutions/trow)

- prepare config file: harbor.yaml
```yaml
hostname: harbor.hitscanner.xyz
# https related config
https:
  # https port for harbor, default is 443
  port: 443
  # The path of cert and key files for nginx
  # important! use fullchain.pem instead of cert.pem
  certificate: /etc/letsencrypt/live/harbor.hitscanner.xyz/fullchain.pem
  private_key: /etc/letsencrypt/live/harbor.hitscanner.xyz/privkey.pem
```
- command
```bash
./run.sh -n master -c ~/harbor -l harbor.yaml
```

## 3. 集群搭建
- k3s
    - what? lightweight k8s by rancher
    - why? lightweight
- setup
    - command
    ```bash
    ./run.sh setup
    ./run.sh -n master setup-server
    ```
    - what's under the hood:
        - install docker (use docker instead of containerd for container solution)
        - install IPVS (use IPVS [mode for kube-proxy](https://docs.tigera.io/calico/latest/networking/configuring/use-ipvs#:~:text=Kube%2Dproxy%20runs%20in%20three,userspace%2C%20iptables%2C%20and%20ipvs.))
        - install NFS-server (for provisioning persistent volumes)
        - install [k3sup](https://github.com/alexellis/k3sup)
        - enable ports on firewall
        - (master only) edit /etc/exports for nfs-server
        - (master only) export KUBECONFIG=/etc/rancher/k3s/k3s.yaml

- install k3s on master
    - command
    ```bash
    ./run.sh -n master install-server
    ```
    - what's under the hood:
        - add cluster public key
        - send cluster private key to remote as default private key
        - run k3s server
            - disable default ingress traefik (will use ingress-nginx instead)
            - disable default lb servicelb a.k.a [klipper-lb](https://github.com/k3s-io/klipper-lb), will use [metallb](https://github.com/metallb/metallb) instead
            - set container engine to docker
            - set proxy-mode to ipvs        
- install k3s on agent
    - command
    ```bash
    ./run.sh -n us-0001,jp-0001,us-0002,sg-0001 install-agent -s master -p 4444
    ```
    - what's under the hood:
        - server ssh parameters: server_ip, server_port
        - set container engine to docker
        - set proxy-mode to ipvs
- label nodes
```bash
kubectl label node master myrole=master
```

## 4. 负载均衡
- layer4 load balancing: metallb
    - why metallb
        - 不使用云提供商的专用负载均衡器，而使用纯软件实现的metallb
        - 适用于bare-metal环境
    - metallb
    ```bash
    helm repo add metallb https://metallb.github.io/metallb
    helm repo update
    ```
    - IP address pool
        - resource definition
    ```yaml
    apiVersion: metallb.io/v1beta1
    kind: IPAddressPool
    metadata:
    name: first-pool
    namespace: metallb-system
    spec:
    addresses:
    - your_second_ip_address/32
    ```
        - why use second address: need first address for in cluster communication.

    - install
    ```bash
    # run on manager
    helm install metallb metallb/metallb -n metallb-system --create-namespace --set nodeSelector.myrole=manager 
    ```
- layer7 load balancing: ingress-nginx controller
    - 使用k8s社区开发的 [ingress-nginx](https://github.com/kubernetes/ingress-nginx)而不是nginx开发的nginx-ingress (二者不是一个东西)
    - install
    ```
    helm repo add ingress-nginx https://kubernetes.github.io/ingress-nginx
    helm repo update
    helm install ingress-nginx ingress-nginx/ingress-nginx -n ingress-nginx/ingress-nginx --create-namespace --set nodeSelector.myrole=manager 
    ```

## 5. 证书管理（可选）
- cert-manager
```bash
helm repo add jetstack https://charts.jetstack.io
helm repo update
```

- config
```bash
helm show values jetstack/cert-manager >cert-manager-values.yaml
# disable startup api check
# important! securePort should be set to 10260 to avoid conflict. Also installCRDs=true is necessary
helm install cert-manager jetstack/cert-manager --namespace cert-manager --create-namespace --version v1.9.0 --set startupapicheck.timeout=5m --set installCRDs=true --set webhook.hostNetwork=true --set webhook.securePort=10260 --values cert-manager.yaml
```

- issuer
[ref](https://cert-manager.io/docs/tutorials/acme/nginx-ingress/)
```
kubectl apply -f issuer-staging.yaml
# or
kubectl apply -f issuer-production.yaml
```

- verify installation with command line tool
```
curl -fsSL -o cmctl.tar.gz https://github.com/cert-manager/cert-manager/releases/download/v1.11.1/cmctl-linux-amd64.tar.gz
cmctl check api
```

## 6. 存储供应
- nfs-client volume provisoner
```bash
helm repo add nfs-subdir-external-provisioner https://kubernetes-sigs.github.io/nfs-subdir-external-provisioner/
helm repo update
helm install -n kube-system nfs-subdir-external-provisioner nfs-subdir-external-provisioner/nfs-subdir-external-provisioner --set nfs.server=server_ip_address --set nfs.path=/home/nfsshare/ --set nodeSelector.myrole=manager
```
- why nfs provisioner
    - 因为需要用到read-write many (多node同时访问)

## 7. 应用部署
- 平台搭建: [airflow](https://github.com/airflow-helm)
    - mysql
        - bitnami version of mysql
        ```bash
        helm repo add bitnami https://charts.bitnami.com/bitnami
        helm repo update
        helm show values bitnami/mysql >k8s/mysql-values.yaml
        # modify auth.user, auth.database, auth.password
        ```
    - install airflow
    ```bash
    helm repo add airflow-stable https://airflow-helm.github.io/charts
    helm repo update
    helm show values bitnami/mysql >k8s/mysql-values.yaml
    MYSQL_ROOT_PASSWORD=$(kubectl get secret --namespace mysql mysql -o jsonpath="{.data.mysql-root-password}" | base64 -d)
    helm install airflow-cluster airflow-stable/airflow -n airflow-cluster --create-namespace --values k8s/airflow-values.yaml --set airflow.config.AIRFLOW__CORE__PARALLELISM=70 --set airflow.config.AIRFLOW__CORE__DAG_CONCURRENCY=70 --set airflow.config.AIRFLOW__CORE__MAX_ACTIVE_RUNS_PER_DAG=70 
    ```
    - config
        - config：
            默认情况下airflow最大并发任务数是16，如果测量点多于16需要修改配置
        ```
        env:
            - name: "AIRFLOW__CORE__PARALLELISM"
            value: "70"
            - name: "AIRFLOW__CORE__DAG_CONCURRENCY"
            value: "70"
            - name: "AIRFLOW__CORE__MAX_ACTIVE_RUNS_PER_DAG"
            value: "70"
        ```
        - command
        ```bash
        helm show values airflow-stable/airflow >k8s/airflow-values.yaml
        helm install airflow-cluster airflow-stable/airflow -n airflow-cluster --create-namespace --values k8s/airflow-values.yaml
        kubectl create secret tls airflow-tls -n airflow-cluster --key <private key filename> --cert <certificate filename>
        ```
        - what's under the hood
            - set executor to KubernetesExecutor
            - change secrets: airflow.fernetKey, airflow.webserverSecretKey, airflow.users.password
            - make sure scheduler, web, runs on master (since master node are more powerful for our use case)
            - disable triggerer, flower, pgbouncer,  (since we don't need them for now)
            - enable log and dags persistence
            - enable ingress for web server
            - disable postgresql and use externalDatabase
            - disable redis since we aren't using celery
    - pvc
        - create a nfs volume for measurement tasks
        - command
        ```bash
        kubectl apply -f k8s/pvc.yaml
        ```
- 应用部署:
    - ETL application:
        - Extract: measurement scamper, iffinder
        - Transform: Rust based high performance data process
        - Load: backed-up to cloud drives
    - dockerize
        - command
        ```bash
        docker build -t harbor.hitscanner.xyz/library/scanner:v1 .
        docker push harbor.hitscanner.xyz/library/scanner:v1
        ```
    - dags: ETL as pipeline on top of airflow
        - dags/traceroute
        ```bash
        airflow dags trigger 'traceroute' --conf $(cat example.json | jq -c .)
        ```

## 8. 持续集成