import json,sys
d=json.load(open(sys.argv[1]+'/tokens.json'))
th=[t['id'] for t in d['color']['themes']]
def val(t,th_):
    v=t['value']; v=v if isinstance(v,str) else v.get(th_,v[th[0]])
    return 'var(--%s)'%v[1:-1] if v.startswith('{') else v
o=['/* Terminal Companion — generated from tokens.json */']
for i,t_ in enumerate(th):
    sel=':root, [data-theme="%s"]'%t_ if i==0 else '[data-theme="%s"]'%t_
    o.append(sel+' {')
    for k in ('color','shadow'):
        for t in d[k]['tokens']:
            if i==0 or isinstance(t['value'],dict): o.append('  --%s: %s;'%(t['name'],val(t,t_)))
    o.append('}')
o.append(':root {')
for k in ('spacing','radius','size'):
    for t in d[k]['tokens']: o.append('  --%s: %s;'%(t['name'],t['value']))
for k,v in d['type']['families'].items(): o.append('  --font-%s: %s;'%(k,v))
o.append('}')
for g in d['type']['groups']:
    for s in g['styles']:
        o.append('.%s { font-family: var(--font-%s); font-size: %s; line-height: %s; font-weight: %s; }'%(s['name'],g['family'],s['fontSize'],s['lineHeight'],s['fontWeight']))
print('\n'.join(o))
