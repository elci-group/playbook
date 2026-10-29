# Front Office

Front Office constructs an eligible portfolio before competition. It never
selects the next player, executes a tool, inspects a repository, discovers live
capabilities or changes readiness. Its constitutional boundary is: no component
may influence the same contest from which it is learning unless it belongs to
the coaching plane.

`playbook-front-office` is a standalone deterministic library. It depends on
core value types and serialization, with no franchise, drive reducer, routing,
Bench, Lastdown or execution-plane dependency. `construct(&PortfolioInput)` has
no execution callback, filesystem path, live task, repository, drive id or
franchise handle. The HTTP adapter passes only its typed request to this pure
function; live dispatch never calls it.

## Restricted evidence

`PortfolioInput` accepts up to sixteen candidates and five abstract workload
classes. `OpponentProfile` contains a fixed task-class enum, workload weight,
novelty, context demand, tool intensity, reasoning depth, coding requirement and
required tool classes. Arbitrary opponent descriptions and unknown fields are
rejected. The representation deliberately cannot carry an actual task request.

Candidates supply historical per-class success and divcon upset statistics,
sample counts, reliability, latency, quota capacity, cost per normalized
workload, price volatility, context limits, tool compatibility, offline status,
provider identity and shared failure domains. Domains should name shared
upstream infrastructure and rate-limit pools as well as providers. Performance
history must declare a season no later than `evidence_through_season`, which
must precede `effective_season`. Concurrent and future observations are rejected.
The host that imports divcon/CERS evidence remains responsible for correct
closed-season provenance; a declared season is not a cryptographic attestation.
There is no automatic feed from current franchise scouting into Front Office.

All money is integer micros in the explicitly selected GBP, USD or EUR unit.
Prices are supplied observations, not live provider quotes. The checked-in
fixture uses synthetic values and capability records, not model recommendations.

## Portfolio selection

The implementation exhaustively compares subsets, bounded at sixteen inputs.
This evaluates counterfactual portfolios, without attempting any live task.
Hard constraints enforce total projected cost, maximum membership, minimum
provider count, per-formation substitution depth, minimum reliability and
historical sample count, latency ceiling, context and tool compatibility,
positive quota, and optional offline fallback for every workload class.

For each formation, fitness combines measured class success, historical divcon
upset performance, reliability, reasoning/coding/tool adequacy and an abstract
novelty discount. The best eligible member contributes coverage; the mean of
the strongest required depth contributes redundancy. Shared provider or failure
domains reduce diversification, even when two providers have different names.
Cost and price volatility reduce portfolio value. Ties prefer lower cost, fewer
members, then stable personnel-id order.

The current integer proxy is:

```text
60 * coverage + 25 * redundancy + 15 * diversification
  - 5 * budget_utilisation - 5 * price_volatility
```

Each term uses parts per million. These are explicit planning heuristics, not
calibrated completion probabilities, a covariance model or live contest results.
Bench coverage is represented by formation eligibility and required depth;
there is no live Bench topology client. Failure-domain independence earns a
portfolio bonus, while minimum provider count is a separate hard constraint.

## Static output and temporal cutover

The output is a checksummed `ROSTER/1` contract with the effective season,
evidence cutoff, input digest, selected ids and providers, formation eligibility,
projected portfolio cost, coverage/redundancy/diversification scores, depth and
diversity requirements, and excluded candidates with reasons. The checksum
detects modification; it is not a signature or an authorization mechanism.

The franchise stores three distinct things:

1. Resource inventory: registered personnel that might serve future rosters.
2. Current and staged contracts: eligibility for kickoff and quarantined plans.
3. Each drive's frozen roster: immutable membership for the entire contest.

Staging a future contract has no tactical effect. Activation is explicit and
requires every current contest to be terminal and reconciled. Active and punted
drives close that window. A contract must advance the current effective season,
and all selected identities and providers must match registered resources.
Failed activation leaves current and staged state unchanged. Nothing activates
automatically after completion or on a daily clock.

At kickoff the drive copies the current contract and eligible personnel. It
uses that frozen copy after substitutions, punts, possession renewals and host
recovery. Front Office does not run on any of those paths. Coaching may change
readiness or select a substitute already inside eligibility. Acquiring a new
resource, staging another contract or learning new results cannot add a player
to the running contest. Legacy manually assembled rosters freeze the inventory
at kickoff until the host adopts explicit contracts.

## HTTP usage

Build the artifact independently of any franchise:

```bash
curl -sS http://127.0.0.1:8088/v1/front-office/rosters \
  -H 'Content-Type: application/json' \
  --data-binary @fixtures/front_office.json > /tmp/roster.json
```

`POST /v1/franchises/{f}/roster-contracts/stage` accepts
`{"contract": <ROSTER/1 object>, "reason": "next season"}`.
`POST /v1/franchises/{f}/roster-contracts/activate` accepts
`{"reason": "between contests"}`. Both use the existing durable franchise
store. Inventory registration and coaching readiness retain their existing
routes. The separate Front Office endpoint neither receives nor looks up a
franchise's current execution state.

Tests cover complementary portfolios, correlated alternatives, hard fallback
constraints, live-payload and same-season rejection, corrupt contracts, cost
overflow, staged isolation, mid-contest activation refusal, frozen eligibility
across recovery and punts, coaching substitution, and legacy snapshot migration.
