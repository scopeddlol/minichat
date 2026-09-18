import sys, json
print([c['name'] for c in json.load(sys.stdin)])
