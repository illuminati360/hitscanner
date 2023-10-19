#!/bin/bash

usage(){
  echo "./run.sh <\$command> <\$args> [\$options]"
  echo "COMMANDS:"
  echo "  -n <\$node_name> <\$operation>"
  echo "    OPERATIONS:"
  echo "      df"
  echo "      get -l <\$local> -r <\$remote>"
  echo "      cat -r <\$remote>"
  echo "      setup"
  echo "      setup-server"
  echo "      install-server"
  echo "      install-agent -s <\$server_node>"
  exit
}

test $# -lt 1 && usage
args=""
while test $# -gt 0; do
  case "$1" in
    -n)
      NODE=$2
      shift 2
      ;;
    -s)
      SERVER=$2
      shift 2
      ;;
    -p)
      PASSWORD=$2
      shift 2
      ;;
    -u)
      USERNAME=$2
      shift 2
      ;;
    -i)
      INPUT=$2
      shift 2
      ;;
    -o)
      OUTPUT=$2
      shift 2
      ;;
    -l)
      LOCAL=$2
      shift 2
      ;;
    -r)
      REMOTE=$2
      shift 2
      ;;
    -c)
      CONFIG=$2
      shift 2
      ;;
    -d)
      CWD=$2
      shift 2
      ;;
    *)
      args="$args $1"
      shift
      ;;
  esac
done
eval set -- "$args"

cmd=$1
test ! -z "$NODE" && limit="-l $NODE"
case $cmd in
  "df")
    ANSIBLE_STDOUT_CALLBACK=yaml ansible-playbook vps.yaml $limit -f 70 --tag df
    ;;
  "cat")
    ANSIBLE_STDOUT_CALLBACK=yaml ansible-playbook vps.yaml $limit -f 70 --tag cat -e "remote=$REMOTE"
    ;;
  "get")
    test -z "$NODE" && exit
    ANSIBLE_STDOUT_CALLBACK=yaml ansible-playbook vps.yaml $limit -f 70 --tag get -e "local=$LOCAL remote=$REMOTE"
    ;;
  "pull")
    test -z "$NODE" && exit
    ANSIBLE_STDOUT_CALLBACK=yaml ansible-playbook vps.yaml $limit -f 70 --tag pull -e "remote=$REMOTE"
    ;;
  "setup")
    ANSIBLE_STDOUT_CALLBACK=yaml ansible-playbook vps.yaml $limit -f 70 --tag setup
    ;;
  "setup-server")
    test -z "$NODE" && exit
    ANSIBLE_STDOUT_CALLBACK=yaml ansible-playbook vps.yaml $limit -f 70 --tag setup-server
    ;;
  "update-server")
    test -z "$NODE" && exit
    ANSIBLE_STDOUT_CALLBACK=yaml ansible-playbook vps.yaml $limit -f 70 --tag update-server
    ;;
  "install-server")
    ANSIBLE_STDOUT_CALLBACK=yaml ansible-playbook vps.yaml $limit -f 70 --tag install-server
    ;;
  "install-agent")
    test -z "$SERVER" && exit
    ANSIBLE_STDOUT_CALLBACK=yaml ansible-playbook vps.yaml $limit -f 70 --tag install-agent -e "server=$SERVER server_port=$PASSWORD"
    ;;
  "uninstall-k3s")
    ANSIBLE_STDOUT_CALLBACK=yaml ansible-playbook vps.yaml $limit -f 70 --tag uninstall-k3s
    ;;
  "mysql")
    test -z "$NODE" && exit
    test -z "$PASSWORD" && exit
    test -z "$USERNAME" && exit
    ANSIBLE_STDOUT_CALLBACK=yaml ansible-playbook vps.yaml $limit -f 70 --tag mysql -e "password=$PASSWORD username=$USERNAME"
    ;;
  "redis")
    test -z "$NODE" && exit
    ANSIBLE_STDOUT_CALLBACK=yaml ansible-playbook vps.yaml $limit -f 70 --tag redis -e "password=$(openssl rand 60 | openssl base64 -A)"
    ;;
  "harbor")
    test -z "$NODE" && exit
    test -z "$CWD" && exit
    test -z "$LOCAL" && exit
    ANSIBLE_STDOUT_CALLBACK=yaml ansible-playbook vps.yaml $limit -f 70 --tag harbor -e "cwd=$CWD local=$LOCAL"
    ;;
  "*")
    usage
    exit
    ;;
esac
