"""Browser-only rendering checks; real broker integration is tested separately."""
import json
import time


def verify_pairing_ui(page, app, output, org="notebook-fixture", project="sheets"):
    errors=[]
    page.on("pageerror",lambda error:errors.append(str(error)))
    state={"connection":"00000000-0000-4000-8000-000000000098","state":"pairing","identity":None,
           "qr_svg":'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 25 25"><rect width="25" height="25" fill="white"/><path d="M4 4h7v7H4zM14 4h7v7h-7zM4 14h7v7H4z" fill="black"/></svg>',
           "expires_at":int(time.time())+60,"error":None}
    def reply(route):route.fulfill(status=200,content_type="application/json",body=json.dumps(state))
    page.route("**/api/devices/pair",reply)
    page.route("**/api/devices/status",reply)
    page.goto(f"{app}/orgs/{org}/projects/{project}/settings/interfaces",wait_until="networkidle")
    page.get_by_role("button",name="Connect with QR code",exact=True).click()
    image=page.get_by_role("img",name="Scan this QR code with the linked devices screen on your phone")
    image.wait_for(state="visible")
    assert image.locator("svg").count()==1
    page.screenshot(path=str(output/"paired-qr-desktop.png"),full_page=True)
    page.set_viewport_size({"width":390,"height":844})
    assert image.bounding_box()["width"]<=390
    assert page.evaluate("document.documentElement.scrollWidth <= window.innerWidth")
    page.screenshot(path=str(output/"paired-qr-mobile.png"),full_page=True)
    state.update(state="connected",identity="fixture-account",qr_svg=None)
    page.get_by_text("Connected as fixture-account",exact=True).wait_for()
    assert image.count()==0
    page.get_by_role("button",name="Use for this project",exact=True).wait_for(state="visible")
    page.get_by_role("button",name="Messaging provider",exact=True).click()
    page.get_by_role("option",name="WhatsApp",exact=True).click()
    page.get_by_role("button",name="Connect with QR code",exact=True).wait_for(state="visible")
    assert page.get_by_text("Connected as fixture-account",exact=True).count()==0
    state.update(state="pairing",identity=None,qr_svg='<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 25 25"><path d="M4 4h7v7H4z"/></svg>')
    page.get_by_role("button",name="Connect with QR code",exact=True).click()
    image.wait_for(state="visible")
    state.update(state="expired",qr_svg=None)
    page.get_by_text("The pairing code expired. Unlink this attempt to generate a new code.",exact=True).wait_for()
    assert image.count()==0
    page.unroute("**/api/devices/pair",reply)
    page.unroute("**/api/devices/status",reply)
    page.set_viewport_size({"width":1280,"height":900})
    assert not errors,errors
    print("PASS browser paired devices: both providers, QR rendering, narrow viewport, polling, provider-switch isolation and expired/connected cleanup",flush=True)
