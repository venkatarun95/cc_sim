import json
import subprocess
import sys
from typing import Dict, List


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

    def __init__(self, stats):
        self.stats = stats


def run_simulation(config: Config) -> SimStats:
    # We need the output to be in stdout
    assert(config.config["log"]["stats_file"] is None)
    sim_res = subprocess.run(["cargo", "run", "--release", "--", "stdin"],
                             capture_output=True, text=True,
                             input=json.dumps(config.config))

    if sim_res.returncode != 0:
        print("Error in simulation. Code: %d" % sim_res.returncode,
              file=sys.stderr)
        exit(1)
    out = sim_res.stdout
    return SimStats(json.loads(out))

print(run_simulation(Config.default()))
