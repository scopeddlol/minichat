import sys, json
d = json.load(sys.stdin)
for key in sys.argv[1:]:
    d = d[int(key)] if key.lstrip('-').isdigit() else d[key]
print(d)
