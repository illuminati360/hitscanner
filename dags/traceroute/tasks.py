from airflow.contrib.operators.kubernetes_pod_operator import KubernetesPodOperator
from kubernetes.client import models as k8s
import os
import json

DATA_DIR = '/opt/data'
SCANNER_IMAGE = "harbor.freemre.com/library/scanner:v5"


class TargetGenerationPodOperator(KubernetesPodOperator):
    def __init__(self, *args, **kwargs):
        super().__init__(image=SCANNER_IMAGE, *args, **kwargs)

    def execute(self, context):
        # configs
        conf = context['dag_run'].conf

        # monitors
        monitors = conf.get('monitors', [])
        if len(monitors) <= 0:
            return -1

        # target
        target_file = conf.get('target', {}).get('file', None)
        target_list = conf.get('target', {}).get('list', None)
        if not target_file and not target_list:
            return -1

        # sampling
        sample_method = conf \
            .get('target', {}) \
            .get('sample', {}) \
            .get('method', "UNIFORM")
        sample_density = conf \
            .get('target', {}) \
            .get('sample', {}) \
            .get('density', 24)
        sample_offset = conf \
            .get('target', {}) \
            .get('sample', {}) \
            .get('offset', 0)
        split = conf['target'].get('split', False)

        # sample targets from input (file or list)
        run_id = context['dag_run'].run_id
        task_dir = os.path.join(DATA_DIR, run_id)
        sampled_targets_filepath = os.path.join(task_dir, 'sampled.txt')

        # command
        self.cmds = ["/bin/sh", "-c"]

        # generate target list and split
        if target_file:
            input = "cat %s" % (target_file)
        else:
            input = "echo '%s'" % ('\n'.join(target_list))
        cmd = """
        mkdir -p {root}
        {input} | ipsample -t {method} -d {density} -o {offset} | shuf >{output}
        split --number=r/{monitors} {output} {output}.
        """.format(
            root=task_dir,
            input=input,
            method=sample_method,
            density=sample_density,
            offset=sample_offset,
            output=sampled_targets_filepath,
            monitors=len(monitors)
        )

        # handle monitors
        for monitor in monitors:
            monitor_dir = os.path.join(task_dir, monitor)
            cmd += "mkdir %s\n" % (monitor_dir)

            monitor_targets_filepath = os.path.join(monitor_dir, 'targets.txt')
            if not split:
                cmd += "ln -s %s %s\n" % (sampled_targets_filepath,
                                          monitor_targets_filepath)
            else:
                cmd += "mv $(ls %s.* | head -n 1) %s\n" % (
                    sampled_targets_filepath, monitor_targets_filepath)
        # write xcom
        cmd += "echo '%s' > /airflow/xcom/return.json" % (json.dumps(monitors))
        print(cmd)
        self.arguments = [
            cmd
        ]
        super().execute(context)
        return monitors


class TracerouteKubernetesPodOperator(KubernetesPodOperator):
    def __init__(self, monitor, *args, **kwargs):
        super().__init__(image=SCANNER_IMAGE, hostnetwork=True, *args, **kwargs)
        self.monitor = monitor

    def execute(self, context):
        self.affinity = k8s.V1Affinity(
            node_affinity=k8s.V1NodeAffinity(
                preferred_during_scheduling_ignored_during_execution=[
                    k8s.V1PreferredSchedulingTerm(
                        weight=1,
                        preference=k8s.V1NodeSelectorTerm(
                            match_expressions=[
                                k8s.V1NodeSelectorRequirement(
                                    key="kubernetes.io/hostname", operator="In", values=[self.monitor])
                            ]
                        ),
                    )
                ]
            )
        )

        # configs
        conf = context['dag_run'].conf
        method = conf.get('method', 'udp')
        firstHop = conf.get('firstHop', 1)
        gap = conf.get('gap', 5)
        attempts = conf.get('attempts', 3)
        pps = conf.get('pps', 50)

        # command
        self.cmds = ["/bin/sh", "-c"]
        # self.cmds = ["echo"]

        run_id = context['dag_run'].run_id
        task_dir = os.path.join(DATA_DIR, run_id)
        monitor_dir = os.path.join(task_dir, self.monitor)
        warts_filepath = os.path.join(monitor_dir, 'traceroute.warts')
        monitor_targets_filepath = os.path.join(monitor_dir, 'targets.txt')
        self.arguments = [
            "scamper -c 'trace -P %s -f %d -g %d -q %d' -p %d -O warts -o %s -f %s" % (
                method,
                firstHop,
                gap,
                attempts,
                pps,
                warts_filepath,
                monitor_targets_filepath
            )
        ]
        super().execute(context)


class Trace2LinkKubernetesPodOperator(KubernetesPodOperator):
    def __init__(self, *args, **kwargs):
        super().__init__(image=SCANNER_IMAGE, *args, **kwargs)

    def execute(self, context):
        # configs
        # command
        self.cmds = ["/bin/sh", "-c"]

        run_id = context['dag_run'].run_id
        task_dir = os.path.join(DATA_DIR, run_id)
        self.arguments = [
            "ls %s/*/ -d | while read l; do trace2link $(ls $l/*.warts) >$l/traceroute.links; done" % (
                task_dir
            )
        ]
        super().execute(context)


class LinkMergeKubernetesPodOperator(KubernetesPodOperator):
    def __init__(self, *args, **kwargs):
        super().__init__(image=SCANNER_IMAGE, *args, **kwargs)

    def execute(self, context):
        # command
        self.cmds = ["/bin/sh", "-c"]

        run_id = context['dag_run'].run_id
        task_dir = os.path.join(DATA_DIR, run_id)
        self.arguments = [
            "linkmerge $(ls %s/*/traceroute.links) >%s/traceroute.links" % (task_dir, task_dir)
        ]
        super().execute(context)


class DealiasKubernetesPodOperator(KubernetesPodOperator):
    def __init__(self, *args, **kwargs):
        super().__init__(image=SCANNER_IMAGE, hostnetwork=True, *args, **kwargs)

    def execute(self, context):
        # configs
        conf = context['dag_run'].conf
        pps = conf.get('pps', 50)

        # command
        self.cmds = ["/bin/sh", "-c"]

        run_id = context['dag_run'].run_id
        task_dir = os.path.join(DATA_DIR, run_id)
        iface_filepath = "%s/traceroute.ifaces" % (task_dir)
        self.arguments = [
            "link2iface %s/traceroute.links | sort >%s && " % (task_dir, iface_filepath) +
            "iffinder -c 100 -r %d -o %s/traceroute %s && " % (pps, task_dir, iface_filepath) +
            "cat %s/traceroute.iffout | grep -v '#' | awk '{ if($NF == \"D\") print $1\" \"$2}' | sort -u >%s/aliases" % (
                task_dir, task_dir)
        ]
        super().execute(context)
