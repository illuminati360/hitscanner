from datetime import datetime
from airflow import DAG, XComArg
from airflow.operators.python import PythonOperator
from airflow.contrib.operators.kubernetes_pod_operator import KubernetesPodOperator
from kubernetes.client import models as k8s
from textwrap import dedent
from tasks import DATA_DIR, TargetGenerationPodOperator, TracerouteKubernetesPodOperator, Trace2LinkKubernetesPodOperator, LinkMergeKubernetesPodOperator, DealiasKubernetesPodOperator

# k8s specific
volume_mount = k8s.V1VolumeMount(
    name='trace-data-pv',
    mount_path=DATA_DIR,
    sub_path=None,
    read_only=False)
pvc = k8s.V1PersistentVolumeClaimVolumeSource(claim_name='trace-data-pvc')
volume = k8s.V1Volume(name='trace-data-pv', persistent_volume_claim=pvc)

with DAG(
    "traceroute_dag",
    # These args will get passed on to each operator
    # You can override them on a per-task basis during operator initialization
    default_args={"retries": 0},
    description="traceroute IPv4",
    schedule=None,
    start_date=datetime.utcnow(),
    catchup=False,
    tags=["traceroute"],
) as dag:
    dag.doc_md = __doc__

    # task1: target_generation
    affinity = k8s.V1Affinity(
        node_affinity=k8s.V1NodeAffinity(
            preferred_during_scheduling_ignored_during_execution=[
                k8s.V1PreferredSchedulingTerm(
                    weight=1,
                    preference=k8s.V1NodeSelectorTerm(
                        match_expressions=[
                            k8s.V1NodeSelectorRequirement(
                                key="myrole", operator="In", values=["master"])
                        ]
                    ),
                )
            ]
        )
    )

    target_generation = TargetGenerationPodOperator(
        namespace='airflow-cluster',
        get_logs=True,
        is_delete_operator_pod=True,
        name="split_target",
        task_id="split_target",
        volumes=[volume],
        volume_mounts=[volume_mount],
        dag=dag,
        do_xcom_push=True,
        affinity=affinity,
    )

    target_generation.doc_md = dedent(
        """\
    #### sample and splits the target list
    This task samples and splits the target list
    """
    )

    traceroute = TracerouteKubernetesPodOperator.partial(
        namespace='airflow-cluster',
        get_logs=True,
        is_delete_operator_pod=True,
        name="traceroute",
        task_id="traceroute",
        volumes=[volume],
        volume_mounts=[volume_mount],
        dag=dag,
        do_xcom_push=True,
    ).expand(monitor=XComArg(target_generation))

    trace2link = Trace2LinkKubernetesPodOperator(
        namespace='airflow-cluster',
        get_logs=True,
        is_delete_operator_pod=True,
        name="trace2link",
        task_id="trace2link",
        volumes=[volume],
        volume_mounts=[volume_mount],
        affinity=affinity,
        dag=dag,
    )

    linkmerge = LinkMergeKubernetesPodOperator(
        namespace='airflow-cluster',
        get_logs=True,
        is_delete_operator_pod=True,
        name="linkmerge",
        task_id="linkmerge",
        volumes=[volume],
        volume_mounts=[volume_mount],
        affinity=affinity,
        dag=dag,
    )

    dealias = DealiasKubernetesPodOperator(
        namespace='airflow-cluster',
        get_logs=True,
        is_delete_operator_pod=True,
        name="dealias",
        task_id="dealias",
        volumes=[volume],
        volume_mounts=[volume_mount],
        affinity=affinity,
        dag=dag,
    )

    traceroute >> trace2link >> linkmerge >> dealias
