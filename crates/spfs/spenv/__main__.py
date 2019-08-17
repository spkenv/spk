import sys
import spenv.cli

if __name__ == "__main__":
    code = spenv.cli.main(sys.argv[1:])
    sys.exit(code)
