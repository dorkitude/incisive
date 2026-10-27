// incisive-dataset: turns analysis responses into a 2-D table (a "dataset")
// that charting code can consume, choosing the layout from the response shape.
//
// matrix[0] is the header row: ['Index', <column labels...>]
// matrix[i][0] is the row label (a timeframe, a group value, a step, ...)

const nullList = (n) => Array.from({ length: Math.max(0, n) }, () => null);

export function flatten(obj, prefix = '', out = {}) {
  for (const key of Object.keys(obj || {})) {
    const value = obj[key];
    const path = prefix ? `${prefix}.${key}` : key;
    if (value !== null && typeof value === 'object') flatten(value, path, out);
    else out[path] = value;
  }
  return out;
}

export function valueAtDeepKey(obj, path) {
  const parts = Array.isArray(path) ? path : String(path).split('.');
  let current = obj;
  for (const part of parts) {
    if (current === null || current === undefined || typeof current[part] === 'undefined') return null;
    current = current[part];
  }
  return current;
}

export class Dataset {
  constructor(config = {}) {
    this.matrix = [['Index']];
    this.meta = { type: undefined };
    this.config = config;
  }

  data(arr) {
    if (!arguments.length) return this.matrix;
    this.matrix = Array.isArray(arr) ? arr : null;
    return this;
  }

  type(str) {
    if (!arguments.length) return this.meta.type;
    this.meta.type = str ? String(str) : undefined;
    return this;
  }

  // set([columnLabel, rowLabel], value) creates the column/row when missing.
  set(coords, value) {
    if (arguments.length < 2 || !coords || coords.length < 2) {
      throw new Error('Incorrect arguments provided for #set method');
    }
    let col = typeof coords[0] === 'number' ? coords[0] : this.matrix[0].indexOf(coords[0]);
    let row = typeof coords[1] === 'number' ? coords[1] : this.selectColumn(0).indexOf(coords[1]);
    if (col < 0 || typeof this.matrix[0][col] === 'undefined') {
      this.appendColumn(String(coords[0]));
      col = this.matrix[0].length - 1;
    }
    if (row < 0 || typeof this.matrix[row] === 'undefined') {
      this.appendRow(String(coords[1]));
      row = this.matrix.length - 1;
    }
    this.matrix[row][col] = value;
    return this;
  }

  selectColumn(q) {
    const index = typeof q === 'number' ? q : this.matrix[0].indexOf(q);
    if (index < 0 || typeof this.matrix[0][index] === 'undefined') return [];
    return this.matrix.map((row) => row[index]);
  }

  selectRow(q) {
    const index = typeof q === 'number' ? q : this.selectColumn(0).indexOf(q);
    return index > -1 && this.matrix[index] ? this.matrix[index] : [];
  }

  appendColumn(label, input) {
    const header = label !== undefined ? label : null;
    if (typeof input === 'function') {
      this.matrix[0].push(header);
      for (let i = 1; i < this.matrix.length; i++) {
        const cell = input.call(this, this.matrix[i], i);
        this.matrix[i].push(cell === undefined ? null : cell);
      }
      return this;
    }
    let values = input || [];
    while (this.matrix.length - 1 < values.length) this.appendRow(String(this.matrix.length));
    values = values.concat(nullList(this.matrix.length - 1 - values.length));
    this.matrix[0].push(header);
    values.forEach((v, i) => { this.matrix[i + 1][this.matrix[0].length - 1] = v; });
    return this;
  }

  appendRow(label, input) {
    const row = [label !== undefined ? label : null];
    if (typeof input === 'function') {
      for (let i = 1; i < this.matrix[0].length; i++) {
        const cell = input.call(this, this.selectColumn(i), i);
        row.push(cell === undefined ? null : cell);
      }
      this.matrix.push(row);
      return this;
    }
    let values = input || [];
    while (this.matrix[0].length - 1 < values.length) this.appendColumn(String(this.matrix[0].length));
    values = values.concat(nullList(this.matrix[0].length - 1 - values.length));
    this.matrix.push(row.concat(values));
    return this;
  }

  deleteColumn(q) {
    const index = typeof q === 'number' ? q : this.matrix[0].indexOf(q);
    if (index > -1 && typeof this.matrix[0][index] !== 'undefined') {
      this.matrix.forEach((row) => row.splice(index, 1));
    }
    return this;
  }

  deleteRow(q) {
    const index = typeof q === 'number' ? q : this.selectColumn(0).indexOf(q);
    if (index > -1 && typeof this.matrix[index] !== 'undefined') this.matrix.splice(index, 1);
    return this;
  }

  updateColumn(q, input) {
    const index = typeof q === 'number' ? q : this.matrix[0].indexOf(q);
    if (index < 0) return this;
    for (let i = 1; i < this.matrix.length; i++) {
      if (typeof input === 'function') {
        const v = input.call(this, this.matrix[i][index], i, this.matrix[i]);
        if (v !== undefined) this.matrix[i][index] = v;
      } else if (Array.isArray(input) && i - 1 < input.length) {
        this.matrix[i][index] = input[i - 1];
      }
    }
    return this;
  }

  updateRow(q, input) {
    const index = typeof q === 'number' ? q : this.selectColumn(0).indexOf(q);
    if (index < 0 || !this.matrix[index]) return this;
    const row = this.matrix[index];
    for (let i = 1; i < row.length; i++) {
      if (typeof input === 'function') {
        const v = input.call(this, row[i], i, row);
        if (v !== undefined) row[i] = v;
      } else if (Array.isArray(input) && i - 1 < input.length) {
        row[i] = input[i - 1];
      }
    }
    return this;
  }

  // Sort body rows by their label (or by a comparator returning a sortable value).
  sortRows(direction = 'asc', valueOf = (row) => row[0]) {
    const head = this.matrix.slice(0, 1);
    const body = this.matrix.slice(1);
    body.sort((a, b) => {
      const va = valueOf(a);
      const vb = valueOf(b);
      if (va === vb) return 0;
      return (va > vb ? 1 : -1) * (direction === 'asc' ? 1 : -1);
    });
    this.matrix = head.concat(body);
    return this;
  }

  // Parser factory: Dataset.parser('interval', 'timeframe.end')(response)
  static parser(name, ...args) {
    const factory = PARSERS[name];
    if (!factory) throw new Error('Requested parser does not exist');
    return factory.apply(this, args);
  }
}

const timeIndex = (record, indexBy) => (indexBy === 'timeframe.end' ? record.timeframe.end : record.timeframe.start);

function groupLabel(record) {
  let label;
  Object.keys(record).forEach((key) => {
    if (key !== 'result') label = key;
  });
  return label;
}

const PARSERS = {
  metric: () => (res) => new Dataset().set(['Value', 'Result'], res.result).type('metric'),

  interval: (indexBy) => (res) => {
    const ds = new Dataset().type('interval');
    (res.result || []).forEach((record) => ds.set(['Result', timeIndex(record, indexBy)], record.value));
    return ds;
  },

  'grouped-metric': () => (res) => {
    const ds = new Dataset().type('grouped-metric');
    (res.result || []).forEach((record) => ds.set(['Result', String(record[groupLabel(record)])], record.result));
    return ds;
  },

  'grouped-interval': (indexBy) => (res) => {
    const ds = new Dataset().type('grouped-interval');
    (res.result || []).forEach((record) => {
      const index = timeIndex(record, indexBy);
      if (record.value && record.value.length) {
        record.value.forEach((group) => ds.set([String(group[groupLabel(group)]), index], group.result));
      } else {
        ds.appendRow(index);
      }
    });
    return ds;
  },

  'double-grouped-metric': (groupBy) => {
    if (!groupBy) throw new Error('Requested parser requires a sequential list (array) of properties to target as a second argument');
    return (res) => {
      const ds = new Dataset().type('double-grouped-metric');
      (res.result || []).forEach((record) => ds.set(['Result', `${record[groupBy[0]]} ${record[groupBy[1]]}`], record.result));
      return ds;
    };
  },

  'double-grouped-interval': (groupBy, indexBy) => {
    if (!groupBy) throw new Error('Requested parser requires a sequential list (array) of properties to target as a second argument');
    return (res) => {
      const ds = new Dataset().type('double-grouped-interval');
      (res.result || []).forEach((record) => {
        const index = timeIndex(record, indexBy);
        (record.value || []).forEach((v) => ds.set([`${v[groupBy[0]]} ${v[groupBy[1]]}`, index], v.result));
      });
      return ds;
    };
  },

  funnel: () => (res) => {
    let result;
    let steps;
    if (Array.isArray(res.result) && res.steps !== undefined) {
      result = res.result; // ad-hoc funnel
      steps = res.steps;
    } else if (res.result && Array.isArray(res.result.result) && res.result.steps !== undefined) {
      result = res.result.result; // saved funnel
      steps = res.result.steps;
    }
    const ds = new Dataset().type('funnel');
    ds.appendColumn('Step Value');
    (result || []).forEach((value, i) => {
      if (steps && steps[i]) ds.appendRow(String(steps[i].event_collection), [value]);
    });
    return ds;
  },

  list: () => (res) => {
    const ds = new Dataset().type('list');
    (res.result || []).forEach((value, i) => ds.set(['Result', String(i + 1)], value));
    return ds;
  },

  extraction: (options = {}) => (res) => {
    const ds = new Dataset().type('extraction');
    const rows = res.result || [];
    const keys = new Set();
    rows.forEach((row) => Object.keys(flatten(row)).forEach((k) => keys.add(k)));
    const names = options.columns || Array.from(keys);
    ds.matrix = [['Index'].concat(names)];
    rows.forEach((row, i) => ds.matrix.push([String(i + 1)].concat(names.map((n) => valueAtDeepKey(row, n)))));
    // Extraction tables have no index column.
    ds.deleteColumn(0);
    return ds;
  },

  'heatmap-axis': () => (res) => {
    const ds = new Dataset().type('heatmap');
    (res.result || []).forEach((value) => {
      const keys = Object.keys(value);
      ds.appendRow([String(value[keys[0]]), String(value[keys[1]]), value.result]);
    });
    return ds;
  },
};

// Picks a parser for a response, preferring the echoed `query` and falling back
// to sniffing the result shape. Returns { parser, args } or { parser: null }.
export function detectParser(response, { indexBy, type } = {}) {
  const query = {
    analysis_type: null, event_collection: null, group_by: null, interval: null, ...(response.query || {}),
  };
  let parser = null;
  const args = [];
  const multiGroup = Array.isArray(query.group_by) && query.group_by.length > 1;
  if (query.analysis_type === 'funnel') parser = 'funnel';
  else if (query.analysis_type === 'extraction') parser = 'extraction';
  else if (query.analysis_type === 'select_unique') {
    if (!query.group_by && !query.interval) parser = 'list';
  } else if (query.analysis_type) {
    if (!query.group_by && !query.interval) parser = 'metric';
    else if (query.group_by && !query.interval) {
      parser = multiGroup ? 'double-grouped-metric' : 'grouped-metric';
      if (multiGroup) args.push(query.group_by);
    } else if (query.interval && !query.group_by) {
      parser = 'interval';
      args.push(indexBy);
    } else {
      parser = multiGroup ? 'double-grouped-interval' : 'grouped-interval';
      if (multiGroup) args.push(query.group_by);
      args.push(indexBy);
    }
  }

  if (!parser) {
    const result = response.result;
    if (typeof result === 'number') parser = 'metric';
    if (Array.isArray(result) && result.length > 0) {
      const first = result[0];
      if (first && first.timeframe && (typeof first.value === 'number' || first.value === null)) {
        parser = 'interval';
        args.push(indexBy);
      }
      if (first && typeof first.result === 'number') parser = 'grouped-metric';
      if (type === 'heatmap' && first && typeof first.result === 'number') parser = 'heatmap-axis';
      if (first && Array.isArray(first.value)) {
        parser = 'grouped-interval';
        args.length = 0;
        args.push(indexBy);
      }
      if (typeof first === 'number' && response.steps !== undefined) parser = 'funnel';
      if ((typeof first === 'string' || typeof first === 'number') && response.steps === undefined) parser = 'list';
      if (!parser) parser = 'extraction';
    }
    if (!parser) parser = 'extraction';
  }
  return { parser, args, query };
}

export function defaultChartType(parser) {
  switch (parser) {
    case 'metric': return 'metric';
    case 'interval': return 'area';
    case 'grouped-metric':
    case 'double-grouped-metric': return 'bar';
    case 'grouped-interval':
    case 'double-grouped-interval': return 'line';
    case 'funnel': return 'horizontal-bar';
    default: return 'table';
  }
}

export function defaultTitle(query = {}) {
  const analysis = query.analysis_type ? query.analysis_type.replace('_', ' ') : '';
  let title = analysis.replace(/\b./g, (a) => a.toUpperCase());
  if (query.event_collection) title += ` - ${query.event_collection}`;
  return title;
}

// parseResponse(response, { indexBy, type, title, columns })
// -> { dataset, parser, type, title, executionMetadata }
export function parseResponse(response, options = {}) {
  const { parser, args, query } = detectParser(response, options);
  const meta = response.metadata || {};
  const factoryArgs = parser === 'extraction' ? [{ columns: options.columns }] : args;
  const dataset = Dataset.parser(parser, ...factoryArgs)({ ...response, query });
  if (parser.indexOf('interval') > -1) {
    dataset.updateColumn(0, (value) => (value === 'Index' ? value : new Date(value)));
  }
  const type = options.type || (meta.visualization && meta.visualization.chart_type) || defaultChartType(parser);
  const title = options.title !== undefined ? options.title : (meta.display_name || defaultTitle(query));
  return {
    dataset,
    parser,
    type,
    title,
    executionMetadata: response.execution_metadata,
  };
}

export default Dataset;
