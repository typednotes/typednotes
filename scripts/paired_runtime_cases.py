"""Actual compiled Lun effect handlers calling the native paired broker path."""
import copy
import hashlib
import hmac
import json
from pathlib import Path
import struct
import time
import uuid
from urllib.request import Request, urlopen

ROOT = Path(__file__).resolve().parents[1]


def verify_paired_runtime(cases, runtime, broker, key, vault, org, user, scratch, command, env, exchange):
    repo = scratch / "paired-runtime"
    repo.mkdir()
    (repo/"lean-toolchain").write_text((ROOT.parent/"lun/lean-toolchain").read_text())
    (repo/"lakefile.toml").write_text(f'name="paired_runtime_fixture"\n[[require]]\nname="linen"\npath="{ROOT.parent/"linen"}"\n[[lean_lib]]\nname="PairedRuntime"\n')
    declarations = ["import Linen.Control.Monad.Effect.Connector", "namespace PairedRuntime", "open Control.Monad.Effect"]
    functions = []
    grants = {}
    for provider, connection in cases:
        declarations += [f'abbrev {provider} : Connector.Capability := {{provider := "{provider}", connection := "{connection}", scopes := [{{operation := "messages.read", root := ["sender", "alice"], descendants := false}}]}}',
                         f'def read_{provider} : Eff [Connector.Connector {provider}] Lean.Json := Connector.call "messages.read" ["sender", "alice"]']
        functions.append({"name":f"read_{provider}","module":"PairedRuntime","function":f"PairedRuntime.read_{provider}",
                          "signature":f"Eff [Control.Monad.Effect.Connector.Connector PairedRuntime.{provider}] Lean.Json"})
        permissions={"scopes":[{"operation":"messages.read","root":["sender","alice"],"descendants":False}],"maxRequestBytes":1048576,"maxResponseBytes":16777216}
        cap={**permissions,"provider":provider,"connection":connection}
        ident,run=str(uuid.uuid4()),str(uuid.uuid4())
        lp=lambda text:struct.pack(">Q",len(text.encode()))+text.encode()
        expires=int(time.time())+600
        caveats=[({"kind":"expiresAt","value":str(expires)},b"\x00"+struct.pack(">Q",expires)),
                 ({"kind":"capability","provider":provider,"action":"messages.read"},b"\x01"+lp(provider)+lp("messages.read")),
                 ({"kind":"resource","value":connection},b"\x02"+lp(connection)),
                 ({"kind":"budget","value":"0"},b"\x03"+struct.pack(">Q",0)),
                 ({"kind":"runId","value":run},b"\x04"+lp(run))]
        tag=hmac.new(bytes.fromhex(key),lp(ident)+lp(org),hashlib.sha256).digest()
        for _,encoded in caveats:tag=hmac.new(tag,encoded,hashlib.sha256).digest()
        warrant={"id":ident,"orgId":org,"tag":tag.hex(),"caveats":[value for value,_ in reversed(caveats)]}
        account=f"{user}/{connection}"
        vault.state["documents"][f"/v1/secret/data/connector-authority/{org}/{run}/{ident}"]={"account":account,"cell":cap,"warrant":cap}
        grants[f"read_{provider}"]=[{"provider":provider,"connection":connection,"account":account,
            "organization":cap,"connectionPermissions":cap,"cell":cap,"warrantPermissions":cap,
            "warrants":[{"operation":"messages.read","warrant":warrant,"cost":0}]}]
    declarations.append("end PairedRuntime")
    (repo/"PairedRuntime.lean").write_text("\n".join(declarations)+"\n")
    command("lake","update",cwd=repo,env=env)
    command("git","init","-q","-b","main",cwd=repo)
    command("git","add","-A",cwd=repo)
    command("git","-c","user.name=fixture","-c","user.email=fixture@localhost","-c","commit.gpgsign=false","commit","-qm","fixture",cwd=repo)
    commit=command("git","rev-parse","HEAD",cwd=repo)
    headers={"Authorization":"Bearer fixture-lun"}
    status,build=exchange(runtime+"/v0/builds",{"source":{"url":repo.as_uri(),"branch":"main","commit":commit},"functions":functions,"graphs":[]},headers)
    assert status==202,build
    description={}
    for _ in range(1500):
        with urlopen(Request(runtime+"/v0/builds/"+build["id"],headers=headers),timeout=10) as response:description=json.load(response)
        if description["state"] in ("ready","failed"):break
        time.sleep(.2)
    assert description["state"]=="ready",description
    context={"binding":{"org_id":org,"user_id":user,"graph_id":str(uuid.uuid4())},
             "policy":{"effects":["Connector"],"providers":["signal","whatsapp"],"domains":[],"tools":[]},
             "liaisonUrl":broker,"connectors":grants}
    for provider,_ in cases:
        name=f"read_{provider}"
        endpoint=runtime+f"/v0/builds/{build['id']}/functions/{name}"
        status,result=exchange(endpoint,context,headers)
        assert status==200 and [m["peer"] for m in result["output"]["body"]["messages"]]==["alice"],result
        denied=copy.deepcopy(context);denied["connectors"][name][0]["cell"]["scopes"]=[]
        before=len(vault.state["reads"])
        status,result=exchange(endpoint,denied,headers)
        assert status==200 and result.get("error"),result
        assert len(vault.state["reads"])==before
        print(f"PASS compiled paired {provider}: real Lun handler → broker → C → Rust; narrowed cell denies before credential reads",flush=True)
