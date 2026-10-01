#!/usr/bin/env python3
"""Bounded hard-feasibility comparison, isolated from product runtime.

CP-SAT has its own assignment model; this does not port SeatTrellis' ten soft
objectives or assert that Rust vs C++ is a controlled language comparison.
"""
from __future__ import annotations

import argparse
from collections import deque
import hashlib
from itertools import combinations
import json
import math
import os
from pathlib import Path
import platform
import statistics
import subprocess
import sys
import time

IMPORT_STARTED = time.perf_counter()
import ortools
from ortools.sat.python import cp_model
ORTOOLS_IMPORT_SECONDS = time.perf_counter() - IMPORT_STARTED
ROOT = Path(__file__).resolve().parent
REPO = Path(os.environ.get('SEATTRELLIS_REPO',
                          str(ROOT.parent.parent) if ROOT.parent.name == 'benchmarks'
                          else '/workspace/seattrellis')).resolve()
RUST = Path(os.environ.get('SEATTRELLIS_RUST_HARNESS',
                           str(ROOT / 'rust-target/release/seattrellis-solver-comparison')))
CLI = Path(os.environ.get('SEATTRELLIS_CLI', str(REPO / 'target/debug/seattrellis')))
SOFT_NAMES = ['vision_front', 'height_back', 'randomize', 'score_balance',
              'score_position', 'score_distribution', 'mentor_pairing',
              'fair_rotation', 'avoid_recent_neighbors', 'cooling']


def corpus_document(n: int, variant: str) -> dict:
    columns = 10
    seats = [{'seat_id': f'S{i:03}', 'row': i // columns, 'col': i % columns,
              'x': i % columns, 'y': i // columns, 'enabled': True}
             for i in range(n)]
    request = {
        'api_version': 2, 'student_count': n,
        'seat_positions': [[s['x'], s['y']] for s in seats],
        'edges': [[i, i+1] for i in range(n-1) if i//columns == (i+1)//columns],
        'fixed_seats': [], 'must_be_adjacent': [], 'cannot_be_adjacent': [],
        'min_distance': [], 'seed': 42, 'time_limit_seconds': 2.0,
        'students': [{'key': f'P{i:03}', 'display_name': f'Student {i}'} for i in range(n)],
        'layout': {'layout_id': f'comparison-{n}', 'name': 'frozen horizontal grid',
                   'seats': seats, 'adjacency': {'include_horizontal': True}},
        'rules': {'seed': 42, 'soft': {name: {'enabled': False} for name in SOFT_NAMES},
                  'groups': []},
    }
    if variant == 'mixed':
        request.update(fixed_seats=[[0, 0], [20, 20]],
                       must_be_adjacent=[[2, 3], [12, 13]],
                       cannot_be_adjacent=[[0, 10], [2, 12], [5, 15], [7, 17]],
                       min_distance=[{'students': [0, n-1], 'distance': 3, 'metric': 'euclidean'},
                                     {'students': [3, 15], 'distance': 2, 'metric': 'graph'}])
        request['rules']['groups'] = [
            {'name': 'separate', 'students': ['P000', 'P010', 'P020'], 'separate': True},
            {'name': 'together', 'students': ['P012', 'P013'], 'together': True},
        ]
    elif variant == 'infeasible':
        # Three mutually adjacent students cannot occupy a horizontal path:
        # this graph has no triangle. This is independently known infeasible.
        request['rules']['groups'] = [
            {'name': 'impossible triangle', 'students': ['P000', 'P001', 'P002'],
             'together': True},
        ]
    return request


def hard_pairs(request: dict):
    must = [tuple(pair) for pair in request['must_be_adjacent']]
    cannot = [tuple(pair) for pair in request['cannot_be_adjacent']]
    index = {student['key']: i for i, student in enumerate(request['students'])}
    for group in request['rules']['groups']:
        members = list(dict.fromkeys(index[key] for key in group['students']))
        pairs = list(combinations(members, 2))
        if group.get('together'): must.extend(pairs)
        if group.get('separate'): cannot.extend(pairs)
    return must, cannot


def distance_table(request: dict):
    count = len(request['seat_positions'])
    adjacency = [set() for _ in range(count)]
    for a, b in request['edges']:
        adjacency[a].add(b)
        adjacency[b].add(a)
    distances = []
    for source in range(count):
        row = [math.inf] * count
        row[source] = 0
        queue = deque([source])
        while queue:
            vertex = queue.popleft()
            for neighbor in adjacency[vertex]:
                if math.isinf(row[neighbor]):
                    row[neighbor] = row[vertex] + 1
                    queue.append(neighbor)
        distances.append(row)
    return distances


def metric_distance(request: dict, distances, a: int, b: int, metric: str):
    return distances[a][b] if metric == 'graph' else math.dist(request['seat_positions'][a], request['seat_positions'][b])


def independent_validate(request: dict, assignment: list):
    assert len(assignment) == request['student_count']
    mapping = dict(assignment)
    assert set(mapping) == set(range(request['student_count']))
    assert len(set(mapping.values())) == len(mapping)
    assert all(0 <= seat < len(request['seat_positions']) for seat in mapping.values())
    edge_set = {tuple(sorted(edge)) for edge in request['edges']}
    must, cannot = hard_pairs(request)
    for student, seat in request['fixed_seats']: assert mapping[student] == seat
    for a, b in must: assert tuple(sorted((mapping[a], mapping[b]))) in edge_set
    for a, b in cannot: assert tuple(sorted((mapping[a], mapping[b]))) not in edge_set
    distances = distance_table(request)
    for rule in request['min_distance']:
        a, b = rule['students']
        assert metric_distance(request, distances, mapping[a], mapping[b], rule['metric']) >= rule['distance']


def cp_sat_run(request: dict):
    started = time.perf_counter()
    model = cp_model.CpModel()
    n, seats = request['student_count'], len(request['seat_positions'])
    assigned = [[model.new_bool_var(f'x_{i}_{s}') for s in range(seats)] for i in range(n)]
    positions = [model.new_int_var(0, seats - 1, f'p_{i}') for i in range(n)]
    for i in range(n):
        model.add_exactly_one(assigned[i])
        model.add(positions[i] == sum(s * assigned[i][s] for s in range(seats)))
    for s in range(seats): model.add_at_most_one(assigned[i][s] for i in range(n))
    for i, s in request['fixed_seats']: model.add(assigned[i][s] == 1)
    directed_edges = sorted({pair for a, b in request['edges'] for pair in ((a, b), (b, a))})
    must, cannot = hard_pairs(request)
    for a, b in must: model.add_allowed_assignments([positions[a], positions[b]], directed_edges)
    for a, b in cannot: model.add_forbidden_assignments([positions[a], positions[b]], directed_edges)
    distances = distance_table(request)
    for rule in request['min_distance']:
        a, b = rule['students']
        allowed = [(s, t) for s in range(seats) for t in range(seats)
                   if metric_distance(request, distances, s, t, rule['metric']) >= rule['distance']]
        model.add_allowed_assignments([positions[a], positions[b]], allowed)
    solver = cp_model.CpSolver()
    solver.parameters.num_search_workers = 1
    solver.parameters.random_seed = 42
    # Give model construction and native search one shared operation budget.
    # JSON decoding is measured separately for both adapters.
    solver.parameters.max_time_in_seconds = max(0.00001, 2.0 - (time.perf_counter() - started))
    validation_error = model.validate()
    assert not validation_error, validation_error
    model_seconds = time.perf_counter() - started
    started = time.perf_counter()
    status = solver.solve(model)
    solve_seconds = time.perf_counter() - started
    started = time.perf_counter()
    assignment = [[i, solver.value(positions[i])] for i in range(n)] if status in (cp_model.OPTIMAL, cp_model.FEASIBLE) else []
    extract_seconds = time.perf_counter() - started
    return {'status': solver.status_name(status), 'assignment': assignment,
            'model_seconds': model_seconds, 'solve_seconds': solve_seconds,
            'extract_seconds': extract_seconds, 'solver_wall_seconds': solver.wall_time,
            'branches': solver.num_branches, 'conflicts': solver.num_conflicts,
            'objective_scope': 'hard_feasibility_only',
            'optimal_means': 'satisfaction solved; no SeatTrellis soft-objective optimality claim'}


def rust_run(path: Path):
    started = time.perf_counter()
    process = subprocess.run([str(RUST), str(path)], capture_output=True, text=True, check=True, timeout=10)
    wall_seconds = time.perf_counter() - started
    result = json.loads(process.stdout)
    result['file'] = path.name
    result['process_wall_seconds'] = wall_seconds
    result['adapter_seconds'] = max(0, wall_seconds - result['solve_seconds'] - result['parse_seconds'] - result['read_seconds'])
    return result


def cli_audit(path: Path, name: str, assignment: list):
    response = {'api_version': 2, 'feasible': True, 'status': 'Solved', 'assignment': assignment,
                'attempts_used': 0, 'hard_constraints_satisfied': True, 'total_cost': 0}
    result_path = ROOT / 'results' / f'{name}.response.json'
    result_path.write_text(json.dumps(response, ensure_ascii=False))
    process = subprocess.run([str(CLI), 'audit', '--problem', str(path), '--solution', str(result_path)],
                             text=True, capture_output=True, check=True, timeout=10)
    report = json.loads(process.stdout)
    assert all(rule['checked'] == rule['satisfied'] for rule in report['hard_rules'].values())
    (ROOT / 'results' / f'{name}.audit.json').write_text(process.stdout)
    return True


def validation_controls(paths: list[Path]):
    path = next(path for path in paths if path.stem == '40-mixed')
    request = json.loads(path.read_text())
    assignment = [[i, i] for i in range(request['student_count'])]
    independent_validate(request, assignment)
    assignment[0][1], assignment[1][1] = assignment[1][1], assignment[0][1]
    try:
        independent_validate(request, assignment)
    except AssertionError:
        python_rejected = True
    else:
        raise AssertionError('independent validator accepted a fixed-seat violation')
    response_path = ROOT / 'results/invalid-fixed.response.json'
    response_path.write_text(json.dumps({'api_version': 2, 'feasible': True,
                                        'status': 'Solved', 'assignment': assignment,
                                        'attempts_used': 0, 'hard_constraints_satisfied': True,
                                        'total_cost': 0}))
    process = subprocess.run([str(CLI), 'audit', '--problem', str(path),
                              '--solution', str(response_path)],
                             capture_output=True, text=True, timeout=10)
    assert process.returncode == 2 and 'hard rule' in process.stderr, process.stderr
    proofs = []
    for path in paths:
        if not path.stem.endswith('-infeasible'): continue
        request = json.loads(path.read_text())
        edges = {tuple(sorted(edge)) for edge in request['edges']}
        triangles = sum(all(tuple(sorted(pair)) in edges for pair in combinations(vertices, 2))
                        for vertices in combinations(range(request['student_count']), 3))
        assert triangles == 0
        proofs.append({'students': request['student_count'], 'seat_graph_triangles': triangles,
                       'required_clique_size': 3, 'independent_infeasibility_proof': True})
    result = {'invalid_fixed_assignment_python_rejected': python_rejected,
              'invalid_fixed_assignment_rust_audit_rejected': True,
              'rust_audit_exit': process.returncode, 'infeasible_graph_controls': proofs}
    (ROOT / 'validation-controls.json').write_text(json.dumps(result, indent=2) + '\n')
    return result


def sha256(path: Path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--worker', type=Path)
    args = parser.parse_args()
    if args.worker:
        started = time.perf_counter()
        request = json.loads(args.worker.read_text())
        parse_seconds = time.perf_counter() - started
        result = cp_sat_run(request)
        result.update(parse_seconds=parse_seconds, import_seconds=ORTOOLS_IMPORT_SECONDS)
        print(json.dumps(result))
        return
    (ROOT / 'corpus').mkdir(exist_ok=True)
    (ROOT / 'results').mkdir(exist_ok=True)
    paths = []
    for count in (40, 60, 80):
        for variant in ('easy', 'mixed', 'infeasible'):
            path = ROOT / 'corpus' / f'{count}-{variant}.json'
            path.write_text(json.dumps(corpus_document(count, variant), ensure_ascii=False, sort_keys=True, indent=2) + '\n')
            subprocess.run([str(CLI), 'validate', '--problem', str(path)], capture_output=True, text=True, check=True)
            paths.append(path)
    controls = validation_controls(paths)
    samples = []
    for trial in range(3):
        for path in paths:
            started = time.perf_counter()
            request = json.loads(path.read_text())
            parse_seconds = time.perf_counter() - started
            # Alternate order to reduce consistent warmup/order bias.
            if trial % 2:
                cp = cp_sat_run(request)
                rust = rust_run(path)
            else:
                rust = rust_run(path)
                cp = cp_sat_run(request)
            cp['parse_seconds'] = parse_seconds
            cp['adapter_seconds'] = parse_seconds + cp['model_seconds'] + cp['extract_seconds']
            cp['combined_seconds'] = cp['adapter_seconds'] + cp['solve_seconds']
            for engine, assignment in [('rust', rust['response']['assignment']), ('cp_sat', cp['assignment'])]:
                if assignment:
                    independent_validate(request, assignment)
                    cli_audit(path, f'{path.stem}-{engine}-{trial}', assignment)
            samples.append({'case': path.stem, 'trial': trial, 'rust': rust, 'cp_sat': cp})
            print(f'{path.stem} trial{trial}: Rust {rust["response"]["status"]} {rust["solve_seconds"]:.5f}s; CP-SAT {cp["status"]} {cp["solve_seconds"]:.5f}s', flush=True)
    summary = []
    for path in paths:
        entries = [sample for sample in samples if sample['case'] == path.stem]
        summary.append({'case': path.stem, 'sha256': sha256(path),
                        'rust_statuses': [entry['rust']['response']['status'] for entry in entries],
                        'cp_sat_statuses': [entry['cp_sat']['status'] for entry in entries],
                        'rust_solve_median_ms': 1000*statistics.median(entry['rust']['solve_seconds'] for entry in entries),
                        'rust_process_median_ms': 1000*statistics.median(entry['rust']['process_wall_seconds'] for entry in entries),
                        'rust_adapter_median_ms': 1000*statistics.median(entry['rust']['adapter_seconds'] for entry in entries),
                        'cp_sat_solve_median_ms': 1000*statistics.median(entry['cp_sat']['solve_seconds'] for entry in entries),
                        'cp_sat_model_median_ms': 1000*statistics.median(entry['cp_sat']['model_seconds'] for entry in entries),
                        'cp_sat_combined_median_ms': 1000*statistics.median(entry['cp_sat']['combined_seconds'] for entry in entries)})
    cold_path = ROOT / 'corpus' / '60-easy.json'
    started = time.perf_counter()
    cold = subprocess.run([sys.executable, str(Path(__file__)), '--worker', str(cold_path)],
                          capture_output=True, text=True, check=True, timeout=15)
    cold_wall = time.perf_counter() - started
    cold_result = json.loads(cold.stdout)
    cold_result['process_wall_seconds'] = cold_wall
    results = {'metadata': {'cpu': next((line.split(':',1)[1].strip() for line in Path('/proc/cpuinfo').read_text().splitlines() if line.startswith('model name')), platform.processor()), 'uname': {key: value for key, value in platform.uname()._asdict().items() if key != 'node'},
                            'python': sys.version, 'ortools': ortools.__version__,
                            'rust_harness_sha256': sha256(RUST), 'cli_sha256': sha256(CLI),
                            'core_source_sha256': {str(path.relative_to(REPO)): sha256(path) for path in sorted((REPO/'crates/seattrellis-core/src').glob('*.rs'))},
                            'rustc': subprocess.check_output(['rustc', '--version'], text=True).strip(),
                            'release_profile': {'opt_level': 3, 'lto': 'thin', 'codegen_units': 1},
                            'num_workers': 1, 'seed': 42, 'deadline_seconds': 2.0, 'deadline_scope': 'model/preparation plus search; JSON decoding measured separately', 'trials': 3,
                            'load_average': os.getloadavg(), 'soft_objectives_ported': [],
                            'limits': ['synthetic horizontal-grid corpus', 'hard feasibility only',
                                       'Python model adapter + native CP-SAT versus release Rust algorithm',
                                       'different algorithms; cannot infer language performance',
                                       'shared cloud host; results are exploratory, no latency SLA']},
               'summary': summary, 'samples': samples, 'cp_sat_cold_process': cold_result,
               'validation_controls': controls}
    (ROOT / 'results.json').write_text(json.dumps(results, ensure_ascii=False, indent=2) + '\n')
    print('Saved', ROOT / 'results.json')


if __name__ == '__main__': main()
