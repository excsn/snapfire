'use server';

let results = null;

export async function testTypes(map, set, date, big) {
  results = [
    {
      label: 'Map',
      ok: map instanceof Map,
      detail: map instanceof Map
        ? 'entries: ' + JSON.stringify([...map.entries()])
        : 'received: ' + typeof map,
    },
    {
      label: 'Set',
      ok: set instanceof Set,
      detail: set instanceof Set
        ? 'values: ' + JSON.stringify([...set])
        : 'received: ' + typeof set,
    },
    {
      label: 'Date',
      ok: date instanceof Date,
      detail: date instanceof Date
        ? date.toISOString()
        : 'received: ' + typeof date,
    },
    {
      label: 'BigInt',
      ok: typeof big === 'bigint',
      detail: typeof big === 'bigint'
        ? big.toString()
        : 'received: ' + typeof big,
    },
  ];
}

export async function getResults() {
  return results;
}
