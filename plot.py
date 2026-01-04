import random
import subprocess
import time
from dataclasses import asdict, dataclass, field
from typing import Dict, List

import matplotlib.pyplot as plt
import networkx as nx
import yaml
from networkx.classes.digraph import DiGraph


@dataclass
class ServiceConfig:
    command: List[str]
    networks: List[str]
    build: str = None


@dataclass
class NetworkConfig:
    driver: str = "bridge"


@dataclass
class DockerComposeConfig:
    services: Dict[str, ServiceConfig] = field(default_factory=dict)
    networks: Dict[str, NetworkConfig] = field(default_factory=dict)


list_of_graph_sizes = [3, 5, 7, 9]


def run_docker_compose(n):
    print(f"Running for N={n}...")
    try:
        subprocess.run(
            "docker-compose down -v --remove-orphans", shell=True, timeout=30
        )
        time.sleep(2)
    except:
        pass
    subprocess.run("docker-compose up -d", shell=True, check=True)
    time.sleep(3)
    start = time.time()
    process = subprocess.Popen(
        "docker-compose logs -f",
        shell=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        bufsize=1,
    )
    try:
        for line in process.stdout:
            print(line, end="", flush=True)
            if "Converged to value." in line:
                break
            if time.time() - start > 300:
                print(f"Timeout after 300s for N={n}")
                break
    finally:
        process.terminate()
        process.wait()
    convergence_time = time.time() - start
    subprocess.run("docker-compose down -v --remove-orphans", shell=True, timeout=30)
    time.sleep(2)
    print(f"N={n}: {convergence_time:.1f}s")
    return convergence_time


subprocess.run("docker build -t peer-base:latest .", shell=True, check=True)

convergence_dict = {}
for graph_size in list_of_graph_sizes:
    undirected_tree = nx.random_spanning_tree(nx.complete_graph(graph_size))
    graph = nx.DiGraph()
    for u, v in undirected_tree.edges():
        if random.random() < 0.5:
            graph.add_edge(u, v)
        else:
            graph.add_edge(v, u)
    config = DockerComposeConfig()
    config.networks.update({"peer-network": NetworkConfig()})
    node_with_value = random.randint(0, graph_size - 1)
    print(f"Node with initial value: {node_with_value}")
    for node in graph.nodes():
        peers = []
        for _, target in graph.edges(node):
            peers.append(f"peer{target}:300{target}")
        value = "1.0" if node_with_value == node else "0.0"
        command_args = [value, str(graph_size), f"0.0.0.0:300{node}"] + peers
        config.services.update(
            {
                f"peer{node}": ServiceConfig(
                    command=command_args, networks=["peer-network"], build="."
                )
            }
        )
    config_dict = {
        "services": {
            name: {k: v for k, v in asdict(service).items() if v is not None}
            for name, service in config.services.items()
        },
        "networks": {
            name: asdict(network) for name, network in config.networks.items()
        },
    }
    with open("docker-compose.yml", "w") as file:
        yaml.dump(config_dict, file, default_flow_style=False, sort_keys=False)
    convergence_dict.update({graph_size: run_docker_compose(graph_size)})

plt.plot(list(convergence_dict.keys()), list(convergence_dict.values()), marker="o")
plt.xlabel("Graph Size")
plt.ylabel("Convergence Time (s)")
plt.title("Convergence Time vs Graph Size")
plt.grid(True)
plt.savefig("convergence_plot.png")
plt.show()
