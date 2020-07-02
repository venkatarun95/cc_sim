from copy import deepcopy
import json
from multiprocessing import Process
from random import randint
import pprint
import subprocess
import sys
from typing import Dict, List, Optional, Tuple


class Config:
    ''' Configuration to be fed into a simulator '''

    def __init__(self, config_dict: Dict) -> None:
        self.config = config_dict

    @classmethod
    def default(cls):
        ''' Is guaranteed to have exactly 1 sender group '''
        return cls({
          "pkt_size": 1500,
          "sim_dur": 100000000,
          "log": {
            "out_terminal": "png",
            "out_file": "test.png",
            "cwnd": "Ignore",
            "rtt": "Ignore",
            "sender_losses": "Ignore",
            "timeouts": "Ignore",
            "link_rates": "Ignore",
            "stats_intervals": [(0, None)],
            "stats_file": None,
            "link_bucket_size": 100000
          },
          "topo": {
            "link": {
              "Const": 15000000
            },
            "bufsize": "Infinite",
            "sender_groups": [
              {
                "num_senders": 1,
                "delay": {
                  "Const": 50000
                },
                "agg_intersend": {
                  "Const": 0
                },
                "cc": "AIMD",
                "start_time": 0,
                "tx_length": "Infinite"
              }
            ]
          },
          "random_seed": 0
        })


class Metadata:
    ''' Metadata about the test being run '''

    def __init__(self, name: str, flags: List[str], description: str = ""):
        self.name = name
        self.flags = flags
        self.description = description


class SimStats:
    ''' Statistics gathered from the simulation '''

    def __init__(self, stats: Dict, config: Config):
        self.stats = stats
        self.config = config

    def summarize(self) -> Dict:
        ''' Summarize the statistics from the simulation '''
        summary = {}

        # Configured periods over which we outputs stats
        periods = self.config.config["log"]["stats_intervals"]
        for (p, period) in enumerate(periods):

            # Find the minimum number of transmission opportunities for this
            # interval in any link (minimum because that will probably be the
            # bottleneck)
            tx_opps = None
            for opps in self.stats["link_tx_ops"].values():
                if tx_opps is None or opps < tx_opps:
                    tx_opps = opps[p]

            period_summary = []

            tot_utilization = 0.
            mean_rtt_sum = 0.
            min_rtt = float("inf")
            num_senders = 0

            # Go through each sender
            for (sender_id, sender_all_periods) in self.stats["sender_stats"].items():
                # Pick the corresponding period in the sender
                sender = None
                for x in sender_all_periods:
                    if tuple(x["config_period"]) == period:
                        sender = x
                        break
                    print(period, x["config_period"])
                assert(sender is not None)

                # We'll collect the summary here
                sender_summary = {"id": sender_id}

                # All packets are these many bytes long
                pkt_size = self.config.config["pkt_size"]

                # Find duration during which packets were transmitted. Use
                # config_period, unless end time is None (meaning infinity) in
                # which case use pkt_period
                start_time = sender["config_period"][0]
                if sender["config_period"][1] is not None:
                    end_time = sender["config_period"][1]
                else:
                    end_time = sender["pkt_period"][1]

                sender_summary["tpt(mbps)"] = pkt_size *\
                    sender["num_cum_acked"] / (end_time - start_time)
                sender_summary["utilization"] = sender["num_cum_acked"]/tx_opps
                sender_summary["mean_rtt(ms)"] = sender["rtt"]["mean"] * 1e-3
                sender_summary["p95_rtt(ms)"] = sender["rtt"]["p95"] * 1e-3
                sender_summary["min_rtt(ms)"] = sender["rtt"]["p0"] * 1e-3
                sender_summary["num_lost"] = sender["num_lost"]

                # Aggregate statistics
                tot_utilization += sender_summary["utilization"]
                mean_rtt_sum += sender_summary["mean_rtt(ms)"]
                min_rtt = min(min_rtt, sender_summary["min_rtt(ms)"])
                num_senders += 1

                period_summary.append(sender_summary)

            summary[period] = {
                "utilization": tot_utilization,
                "mean_rtt(ms)": mean_rtt_sum / num_senders,
                "min_rtt(ms)": min_rtt,
                "per_sender": period_summary
             }
        return summary


class Experiment:
    ''' Base class for specifying experiments '''
    def metadata(cls) -> Metadata:
        raise NotImplementedError()

    def config(cls) -> Config:
        raise NotImplementedError()


def pick_random_link_params(
        config: Config,
        num_senders: Optional[Tuple[int, int]] = (1, 10),
        link: Optional[Tuple[int, int]] = (1500, int(50e6)),
        delay: Optional[Tuple[int, int]] = (int(1e3), int(1e5))
        ) -> Config:
    ''' Return a new config with modified randomly chosen parameters. If any
    range is None, it will not be modified '''
    c = deepcopy(config)

    if num_senders is not None:
        if randint(0, 1) == 0:
            n = 1
        else:
            n = randint(num_senders[0], num_senders[1])
        c.config["topo"]["sender_groups"][0]["num_senders"] = n
    if link is not None:
        c.config["topo"]["link"] = {"Const": randint(*link)}
    if delay is not None:
        c.config["topo"]["sender_groups"][0]["delay"] = randint(*delay)
    return c


class Plain(Experiment):
    def config(cls) -> Config:
        config = Config.default()
        config = pick_random_link_params(config)
        return config


class Aggregated(Experiment):
    def config(cls) -> Config:
        config = Config.default()
        config = pick_random_link_params(config)
        if randint(0, 1):
            agg_rv = {"Const": randint(1000, 100000)}
        else:
            agg_rv = {"Poisson": randint(1000, 100000)}
        config.config["topo"]["sender_groups"][0]["agg_delay"] = agg_rv
        return config


class ShortBuffer(Experiment):
    def config(cls) -> Config:
        config = Config.default()
        config = pick_random_link_params(config)
        # Figure out what BDP we picked
        bdp = config.config["topo"]["sender_groups"][0]["delay"] * 1e-6 * \
            config.config["topo"]["link"]["Const"] / config.config["pkt_size"]
        # Choices for the buffer size
        bufsize = [1, 2, 5, 10, int(bdp * 0.05), int(bdp * 0.1)]
        config.config["topo"]["bufsize"] =\
            {"Finite": max(1, bufsize[randint(0, len(bufsize))])}
        return config


def run_simulation(config: Config) -> SimStats:
    # We need the output to be in stdout

    print(config.config)
    sim_res = subprocess.run(["cargo", "run", "--release", "--", "stdin"],
                             capture_output=True, text=True,
                             input=json.dumps(config.config))

    if sim_res.returncode != 0:
        print("Error in simulation. Code: %d" % sim_res.returncode,
              file=sys.stderr)
        print(sim_res.stderr, file=sys.stderr)
        print(config)
        exit(1)
    out = sim_res.stdout
    return SimStats(json.loads(out), config)


def run_experiment(exp):
    config = exp().config()
    stats = run_simulation(config)
    print(config.config)
    pprint.pp(stats.summarize())


if __name__ == "__main__":
    experiments = [ShortBuffer]  # [Plain, Aggregated]
    processes = []
    for i in range(4):
        for exp in experiments:
            p = Process(target=run_experiment, args=(exp,))
            p.start()
            processes.append(p)
    for p in processes:
        p.join()

if __name__ == "__main_":
    config = Config({'pkt_size': 1500, 'sim_dur': 100000000, 'log': {'out_terminal': 'png', 'out_file': 'test.png', 'cwnd': 'Ignore', 'rtt': 'Ignore', 'sender_losses': 'Ignore', 'timeouts': 'Ignore', 'link_rates': 'Ignore', 'stats_intervals': [(0, None)], 'stats_file': None, 'link_bucket_size': 100000}, 'topo': {'link': {'Const': 15219044}, 'bufsize': 'Infinite', 'sender_groups': [{'num_senders': 6, 'delay': 35740, 'agg_intersend': {'Const': 0}, 'cc': 'AIMD', 'start_time': 0, 'tx_length': 'Infinite'}]}, 'random_seed': 0})
    stats = SimStats(
        {'link_tx_ops': {'0': [2564102]}, 'sender_stats': {'14': [{'config_period': [0, None], 'pkt_period': [65091, 99972015], 'num_cum_acked': 553605, 'num_lost': 0, 'num_timeouts': 0, 'rtt': {'mean': 112706, 'stddev': 32713, 'p0': 64979, 'p25': 81920, 'p50': 115934, 'p95': 159777, 'p99': 163185, 'p100': 163972}, 'cwnd': {'mean': 702, 'stddev': 249, 'p0': 4, 'p25': 526, 'p50': 744, 'p95': 1026, 'p99': 1047, 'p100': 1053}}], '10': [{'config_period': [0, None], 'pkt_period': [65052, 99999978], 'num_cum_acked': 553828, 'num_lost': 0, 'num_timeouts': 0, 'rtt': {'mean': 112716, 'stddev': 32723, 'p0': 64979, 'p25': 81920, 'p50': 115934, 'p95': 159777, 'p99': 163185, 'p100': 163972}, 'cwnd': {'mean': 702, 'stddev': 249, 'p0': 4, 'p25': 526, 'p50': 744, 'p95': 1026, 'p99': 1048, 'p100': 1053}}], '2': [{'config_period': [0, None], 'pkt_period': [64974, 99981648], 'num_cum_acked': 553852, 'num_lost': 0, 'num_timeouts': 0, 'rtt': {'mean': 112698, 'stddev': 32727, 'p0': 64979, 'p25': 81920, 'p50': 115934, 'p95': 159777, 'p99': 163185, 'p100': 163972}, 'cwnd': {'mean': 702, 'stddev': 249, 'p0': 4, 'p25': 526, 'p50': 744, 'p95': 1026, 'p99': 1048, 'p100': 1053}}], '6': [{'config_period': [0, None], 'pkt_period': [65013, 99991281], 'num_cum_acked': 553852, 'num_lost': 0, 'num_timeouts': 0, 'rtt': {'mean': 112709, 'stddev': 32725, 'p0': 64979, 'p25': 81920, 'p50': 115934, 'p95': 159777, 'p99': 163185, 'p100': 163972}, 'cwnd': {'mean': 702, 'stddev': 249, 'p0': 4, 'p25': 526, 'p50': 744, 'p95': 1026, 'p99': 1048, 'p100': 1053}}]}},
        config
    )

    pprint.pp(stats.summarize())
