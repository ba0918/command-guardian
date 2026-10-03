import { test, expect } from "bun:test";
import { BudgetControl } from "../../src/advisor-control.js";

const nonce = "07070707070707070707070707070707";
const probe = JSON.stringify({version:1,nonce,kind:"budget_probe"});
const start = JSON.stringify({version:1,nonce,kind:"advisory_start",timeout_ms:10000});

// @kotowari[REQ-advisor-020, EX-advisor-039, EX-advisor-045]
test("probe_keeps_original_deadline_and_first_valid_notification_extends_from_receive_time", () => {
  const control = new BudgetControl(6000);
  expect(control.receive(probe,120)).toEqual({reply:{version:1,nonce,kind:"budget_reply",original_remaining_ms:5880}});
  expect(control.receive(start,5050)).toEqual({reply:{version:1,nonce,kind:"advisory_ack",accepted:true},deadline:16050});
  expect(control.receive(start,5075)).toBeUndefined();
});

// @kotowari[REQ-advisor-020, EX-advisor-047, EX-advisor-048, EX-advisor-050]
test("reservation_boundary_expired_original_deadline_and_cancelled_control_never_extend", () => {
  for (const now of [5500,6000,6100]) {
    const control = new BudgetControl(6000);
    control.receive(probe,120);
    expect(control.receive(start,now)).toEqual({reply:{version:1,nonce,kind:"advisory_ack",accepted:false}});
    expect(control.receive(start,now+1)).toBeUndefined();
  }
  const cancelled = new BudgetControl(6000);
  cancelled.receive(probe,120);
  cancelled.cancel();
  expect(cancelled.receive(start,5050)).toBeUndefined();
});

// @kotowari[REQ-advisor-020, EX-advisor-046, EX-advisor-047]
test("wrong_version_nonce_duplicate_fields_invalid_timeout_and_no_probe_cannot_negotiate", () => {
  expect(new BudgetControl(6000).receive(start,5000)).toBeUndefined();
  for (const message of [
    start.replace('"version":1','"version":2'),
    start.replace(nonce,"00000000000000000000000000000000"),
    start.replace('"version":1','"version":2,"version":1'),
    start.replace('10000','0'),start.replace('10000','1.5'),start.replace('10000','9007199254740991'),
    start.replace('10000','10000,"extra":true'),
  ]) {
    const control = new BudgetControl(6000);
    control.receive(probe,120);
    expect(control.receive(message,5000)?.deadline).toBeUndefined();
    expect(control.receive(start,5001)).toBeUndefined();
  }
  expect(new BudgetControl(6000).receive(probe,6000)).toBeUndefined();
});
