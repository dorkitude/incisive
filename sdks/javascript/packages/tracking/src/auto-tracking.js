// Automatic browser tracking: page views, link clicks, form submissions,
// element views, plus enrichment add-on instructions for the server.

import { VERSION_TAG } from './version.js';

const DEFAULTS = {
  ignoreDisabledFormFields: false,
  ignoreFormFieldTypes: ['password'],
  recordClicks: true,
  recordClicksPositionPointer: false,
  recordFormSubmits: true,
  recordPageViews: true,
  recordPageViewsOnExit: false,
  recordScrollState: true,
  recordElementViews: true,
  shareUuidAcrossDomains: false,
  collectIpAddress: true,
  collectUuid: true,
  cookieName: 'incisive',
  catchError: undefined,
  collectionNames: {
    pageviews: 'pageviews',
    clicks: 'clicks',
    formSubmissions: 'form_submissions',
    elementViews: 'element_views',
  },
};

// Name of the reserved metadata object in every event. Add-on names and server-side
// placeholders are namespaced with it.
export const RESERVED = 'incisive';
const addon = (name) => `${RESERVED}:${name}`;
const placeholder = (name) => '${' + RESERVED + '.' + name + '}';

const secondsSince = (date) => Math.round((Date.now() - date.getTime()) / 1000);
const msSince = (date) => Date.now() - date.getTime();

// Enrichment instructions understood by the server (stored under the reserved
// metadata object's `addons` list).
export function defaultAddons(collectIpAddress = true) {
  return [
    { name: addon('ua_parser'), input: { ua_string: 'user_agent' }, output: 'tech' },
    { name: addon('url_parser'), input: { url: 'url.full' }, output: 'url.info' },
    { name: addon('url_parser'), input: { url: 'referrer.full' }, output: 'referrer.info' },
    { name: addon('date_time_parser'), input: { date_time: `${RESERVED}.timestamp` }, output: 'time.utc' },
    { name: addon('date_time_parser'), input: { date_time: 'local_time_full' }, output: 'time.local' },
    { name: addon('ip_to_geo'), input: { ip: 'ip_address', remove_ip_property: !collectIpAddress }, output: 'geo' },
  ];
}

export function initAutoTracking(client, lib, userOptions = {}) {
  if (typeof window === 'undefined' || typeof document === 'undefined') {
    client.emit('error', 'Auto-tracking is only available in a browser.');
    return client;
  }
  const options = {
    ...DEFAULTS,
    ...userOptions,
    collectionNames: { ...DEFAULTS.collectionNames, ...(userOptions.collectionNames || {}) },
  };
  const { helpers, utils } = lib;
  const names = options.collectionNames;

  if (client.config.requestType === 'beaconAPI' && options.catchError) {
    throw new Error('The Beacon API ignores responses, so catchError cannot be used with requestType "beaconAPI".');
  }
  // Without an explicit transport, prefer the Beacon API (survives navigation)
  // unless the caller wants to observe errors.
  if (!client._requestTypeExplicit) {
    client.config.requestType = options.catchError ? 'fetch' : 'beaconAPI';
  }

  let visibleSince = new Date();
  let accumulatedS = 0;
  let accumulatedMs = 0;
  if (typeof document.addEventListener === 'function') {
    document.addEventListener('visibilitychange', () => {
      if (document.hidden) {
        accumulatedS += secondsSince(visibleSince);
        accumulatedMs += msSince(visibleSince);
      } else {
        visibleSince = new Date();
      }
    }, false);
  }

  const cookie = new utils.cookie(options.cookieName);
  const domainName = helpers.getDomainName(window.location.hostname);
  const cookieOptions = domainName && options.shareUuidAcrossDomains ? { domain: `.${domainName}` } : {};

  let uuid;
  if (options.collectUuid) {
    uuid = cookie.get('uuid');
    if (!uuid) {
      uuid = helpers.getUniqueId();
      cookie.set('uuid', uuid, cookieOptions);
    }
  }

  let initialReferrer = cookie.get('initialReferrer');
  if (!initialReferrer) {
    initialReferrer = document.referrer || undefined;
    cookie.set('initialReferrer', initialReferrer, cookieOptions);
  }

  let scrollState = {};
  if (options.recordScrollState) {
    scrollState = helpers.getScrollState();
    utils.listener('window').on('scroll', () => {
      scrollState = helpers.getScrollState(scrollState);
    });
  }

  const addons = defaultAddons(options.collectIpAddress);

  client.extendEvents(() => {
    const browserProfile = helpers.getBrowserProfile();
    const now = new Date();
    return {
      tracked_by: VERSION_TAG,
      local_time_full: now.toISOString(),
      user: { uuid },
      page: {
        title: document.title,
        description: browserProfile.description,
        scroll_state: scrollState,
        time_on_page: accumulatedS > 0 ? accumulatedS : secondsSince(visibleSince),
        time_on_page_ms: accumulatedMs > 0 ? accumulatedMs : msSince(visibleSince),
      },
      // Placeholders the server replaces with the request's IP address and User-Agent.
      ip_address: placeholder('ip'),
      geo: {},
      user_agent: placeholder('user_agent'),
      tech: { profile: browserProfile },
      url: { full: window.location.href, info: {} },
      referrer: { initial: initialReferrer, full: document.referrer, info: {} },
      time: { local: {}, utc: {} },
      [RESERVED]: { timestamp: now.toISOString(), addons },
    };
  });

  const record = (collection, event) => {
    const result = client.recordEvent({ collection, event });
    if (options.catchError && result && typeof result.catch === 'function') {
      return result.catch((err) => options.catchError(err));
    }
    if (result && typeof result.catch === 'function') result.catch(() => {});
    return result;
  };

  if (options.recordClicks) {
    utils.listener('a, a *').on('click', function onClick(e) {
      let event = {
        element: helpers.getDomNodeProfile(e.target),
        local_time_full: new Date().toISOString(),
      };
      if (options.recordClicksPositionPointer) {
        event = { ...event, pointer: { x_position: e.pageX, y_position: e.pageY } };
      }
      return record(names.clicks, event);
    });
  }

  if (options.recordFormSubmits) {
    utils.listener('form').on('submit', function onSubmit(e) {
      const el = e.target;
      return record(names.formSubmissions, {
        form: {
          action: el.action,
          fields: utils.serializeForm(el, {
            disabled: options.ignoreDisabledFormFields,
            ignoreTypes: options.ignoreFormFieldTypes,
          }),
          method: el.method,
        },
        element: helpers.getDomNodeProfile(el),
        local_time_full: new Date().toISOString(),
      });
    });
  }

  if (options.recordPageViews && !options.recordPageViewsOnExit) {
    record(names.pageviews, {});
  }

  if (options.recordPageViewsOnExit) {
    window.addEventListener('beforeunload', () => {
      client.config.requestType = 'beaconAPI';
      client.recordEvent({ collection: names.pageviews, event: {} });
    });
  }

  if (options.recordElementViews && typeof IntersectionObserver !== 'undefined') {
    const observer = new IntersectionObserver((entries) => {
      entries.forEach((entry) => {
        if (entry.isIntersecting) {
          record(names.elementViews, {
            element: helpers.getDomNodeProfile(entry.target),
            local_time_full: new Date().toISOString(),
          });
        }
      });
    }, { threshold: 1.0 });
    document.querySelectorAll('.track-element-view').forEach((el) => observer.observe(el));
    client.observers.IntersectionObserver = observer;
  }

  return client;
}
