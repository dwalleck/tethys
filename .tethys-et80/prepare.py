"""Build the existing qualification driver pair from the current repaired source."""
import argparse
import json
from pathlib import Path
import sys

REPOSITORY = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPOSITORY / ".tethys-82a6/oracles"))
from qualification_environment import controlled_environment
from qualification_runtime import Runtime


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    sdk = Path("/home/dwalleck/.local-dotnet/sdk/10.0.102")
    runtime = Runtime(
        REPOSITORY, args.output,
        cli=REPOSITORY / "target/debug/tethys", sdk=sdk,
        companion=REPOSITORY / "target/worker-dist/msbuild-evaluate",
        environment=controlled_environment(
            REPOSITORY / "target/qualification/corpus-environment-v5", sdk),
        timeout=1800,
    )
    try:
        receipt = runtime.prepare()
        path = args.output / "preparation.json"
        path.write_text(json.dumps(receipt, indent=2) + "\n")
        print(path)
    finally:
        runtime.close()


if __name__ == "__main__":
    main()
