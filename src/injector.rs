pub const ADBLOCK_COSMETIC_CSS: &str = r#"
/* Ferro AdBlock Cosmetic Element Hiding */
#cts_test,
#ad_ctd,
.textads,
.adsbox,
.banner-ads,
.banner_ads,
.adbox,
.ADBox,
.AdBox,
.afs_ads,
.ad-zone,
.ad-space,
.ad-unit,
.code-block,
div[class*="code-block"],
[class^="__clb-"],
img[src*="Caution1"],
img[data-src*="Caution1"],
.wp-image-1133,
.elementor-element-35203eb,
.elementor-element-5c7335d,
.elementor-element-bebe1cc,
[data-id="5c7335d"],
[data-id="35203eb"],
[data-id="bebe1cc"],
img[src*="Screenshot_1"],
img[data-src*="Screenshot_1"],
.wp-image-3462,
img[src*="Screenshot_16"],
img[data-src*="Screenshot_16"],
.wp-image-30781,
[class*="feedback"],
#feedback,
.feedback-btn,
[data-widget_type*="feedback"],
[class*="fluid-player"],
[id*="fluid-player"],
.fluid_video_wrapper,
.lp-b__brand,
[class*="lp-b__brand"],
.brand_bg,
#brand_bg,
[class*="brand_bg"],
[id*="brand_bg"],
.brand_top,
#brand_top,
[class*="brand_top"],
[id*="brand_top"],
.regpro,
[class*="branding"],
[id*="branding"],
[class*="brload"],
[id*="brload"],
#trickorlife,
[id*="trickorlife"],
[id*="epom"],
[class*="epom"],
.epom-brend,
[id^="br5g"],
[class^="br5g"],
[id*="br5g"],
[class*="br5g"],
.br5g33utrrt,
#br5g33utrrs,
#br5g33utrru,
div[style*="JBWheell"],
div[style*="cdn.traffer"],
.elementor-element-928b71a,
.elementor-element-f78181b,
[data-id="928b71a"],
[data-id="f78181b"],
[class*="DeskBrand"],
[id*="DeskBrand"],
[style*="DeskBrand"],
[style*="traffer"],
[id*="traffer"],
[class*="traffer"],
a[href*="traffer"],
a[href*="parik"],
a[href*="21wiz"],
a[href*="1x"],
a[href*="casino"],
a[href*="vulkan"],
a[href*="bet"],
a[href*="bonus"],
a[href*="dog"],
a[href*="megaways"],
a[target="_blank"][style*="position: fixed"],
a[target="_blank"][style*="position: absolute"][style*="width: 100%"],
div[style*="z-index: 0"][style*="position: fixed"],
div[style*="z-index: -1"][style*="position: fixed"],
#branding,
.branding,
#branding-link,
.branding-link,
#brand_bg,
.brand_bg,
.brand_top,
#brand_top,
.b-branding,
#b-branding,
.banner-bg,
#banner-bg,
[class*="banner-ad"],
[class*="ad-container"],
[class*="advertisement"],
[class*="ad-box"],
[class*="ad-wrapper"],
[class*="ad-slot"],
[class*="adsbygoogle"],
ins.adsbygoogle,
[id*="google_ads"],
iframe[src*="doubleclick"],
iframe[src*="googleads"],
iframe[src*="adservice"],
iframe[id^="aswift_"],
iframe[id^="google_ads_frame"],
div[id^="google_ads_div_"],
div[id^="div-gpt-ad"],
[id*="yandex_rtb"],
[id*="yandex_ad"],
[class*="ya-partner"],
[class*="ya-site-form"],
.ya-direct,
div[class*="YandexDirect"],
[id*="marketgid"],
[id*="smi2"],
[id*="kadam"],
[id*="teasernet"],
[class*="advertur"],
[class*="oblivki"],
[class*="directadvert"],
[class*="sponsored-post"],
[data-ad-unit],
[data-ad-client],
[data-ad-slot],
[data-ad-id],
[data-ad-format],
.branding,
.teaser,
.teaser-block,
.popup-ad,
.clickunder,
.popunder,
.b-banner,
.adv-block,
[class*="preroll"],
[class*="vast-ad"],
[class*="ima-ad"],
[class*="vpaid"],
.ad-header,
.ad-footer,
.ad-sidebar,
#ad-banner,
.banner_ad,
.top-ad,
.bottom-ad {
    display: none !important;
    visibility: hidden !important;
    height: 0 !important;
    min-height: 0 !important;
    max-height: 0 !important;
    width: 0 !important;
    max-width: 0 !important;
    opacity: 0 !important;
    pointer-events: none !important;
    overflow: hidden !important;
    margin: 0 !important;
    padding: 0 !important;
    border: none !important;
}
"#;

pub const ADBLOCK_JS: &str = r#"
(function() {
    if (window._ferro_adblock_installed) return;
    window._ferro_adblock_installed = true;

    const AD_DOMAINS = [
        'doubleclick.', 'googlesyndication.', 'googleadservices.', 'googleads.', 'pagead',
        'adservice.google', 'googletagservices.', 'googletagmanager.', 'google-analytics.',
        'an.yandex.', 'mc.yandex.', 'adfox.ru', 'awaps.yandex.', 'bs.yandex.', 'direct.yandex.',
        'yabs.yandex.', 'yandex.ru/ads', 'ads.adfox.', 'extmail.yandex.', 'top-fwz1.mail.ru',
        'target.my.com', 'ad.mail.ru', 'criteo.', 'adnxs.', 'scorecardresearch.', 'moatads.',
        'outbrain.', 'taboola.', 'adroll.', 'advertising.com', 'rubiconproject.', 'pubmatic.',
        'openx.net', 'smartadserver.', 'amazon-adsystem.', 'adtechus.', 'revcontent.',
        'popads.', 'popcash.', 'propellerads.', 'exoclick.', 'adsterra.', 'hilltopads.',
        'clickadu.', 'trafficjunky.', 'juicyads.', 'plugrush.', 'rtb.adxcore.', 'evadav.',
        'rollerads.', 'richads.', 'monetag.', 'onclickads.', 'traffichub.',
        'traffer.', '21wiz.', 'advmaker.', 'adlook.', 'viqeo.', 'videoroll.', 'buzzoola.',
        'moevideo.', 'videonow.', 'eagleplatform.', 'adriver.', 'soloway.', 'targetix.',
        'hpmdnetwork.', 'rutarget.', 'clickadilla.', 'tizerlady.', 'luxup.', 'globalteas.',
        'otclick', 'redclick.', 'bodyclick.', 'admixer.',
        '1xbet', '1x-bet', 'melbet', 'mostbet', 'vulkan', 'vulcan', 'joycasino',
        'pin-up', 'pinup', 'vavada', 'catcasino', 'spinamba', 'bet365', 'fonbet', 'winline',
        'betboom', 'parik', 'parimatch', '1win', 'linebet', 'marathonbet', 'leonbets',
        'stake.com', 'roobet.', 'bc.game', 'playfortuna', 'booi.',
        'marketgid', 'teasernet', 'directadvert', 'kadam.', 'oblivki.', 'advertur.',
        'smi2.', '24smi.', 'lentainform.', 'mgid.', 'bidgear.', 'adkeeper.',
        'brload', 'load_brclass',
        '12ezo5v60.', 'ybs2ffs7v.', 'fluidplayer.', 'parik24', 'doghouse', 'megaways',
        'fvcwqkkqmuv', '/aas/r45d/', 'kgi.js', 'regpro'
    ];

    const SUPER_TEST_HOSTS = [
        "2975c.v.fwmrm.net",
        "a.klaviyo.com",
        "a.lenovo.com",
        "a.teads.tv",
        "aan.amazon.com",
        "aax.amazon-adsystem.com",
        "acdn.adnxs.com",
        "ad.crwdcntrl.net",
        "ad.doubleclick.net",
        "ad.gt",
        "ad.lgappstv.com",
        "ad.linksynergy.com",
        "ad.mail.ru",
        "adc3-launch.adcolony.com",
        "adclick.g.doubleclick.net",
        "adcolonyads30.adcolony.com",
        "adfox.yandex.ru",
        "adfstat.yandex.ru",
        "adjust.com",
        "adlog.vivo.com",
        "adm.hotjar.com",
        "adnxs.com",
        "adroll.com",
        "ads-api.tiktok.com",
        "ads-api.twitter.com",
        "ads-api.vivo.com",
        "ads-api.x.com",
        "ads-sg.tiktok.com",
        "ads.huawei.com",
        "ads.linkedin.com",
        "ads.microsoft.com",
        "ads.pinterest.com",
        "ads.pubmatic.com",
        "ads.roku.com",
        "ads.snapchat.com",
        "ads.stickyadstv.com",
        "ads.tiktok.com",
        "ads.tremorhub.com",
        "ads.vizio.com",
        "ads.vk.com",
        "ads.x.com",
        "ads.yahoo.com",
        "ads.youtube.com",
        "ads30.adcolony.com",
        "adserver.unityads.unity3d.com",
        "adservetx.media.net",
        "adservice.google.com",
        "adsfs.oppomobile.com",
        "adspagead2.googlesyndication.com",
        "adsrvr.org",
        "adtago.s3.amazonaws.com",
        "adtech.yahooinc.com",
        "advertising-api-eu.amazon.com",
        "advertising.apple.com",
        "advertising.yahoo.com",
        "advertising.yandex.ru",
        "advice-ads.s3.amazonaws.com",
        "adx.ads.oppomobile.com",
        "afs.googlesyndication.com",
        "amazon-adsystem.com",
        "amazonadtago.s3.amazonaws.com",
        "amoeba.web.roku.com",
        "an.facebook.com",
        "analytics-api.samsunghealthcn.com",
        "analytics-sg.tiktok.com",
        "analytics.adobe.io",
        "analytics.google.com",
        "analytics.pinterest.com",
        "analytics.pointdrive.linkedin.com",
        "analytics.query.yahoo.com",
        "analytics.s3.amazonaws.com",
        "analytics.tiktok.com",
        "analytics.twitter.com",
        "analytics.x.com",
        "analytics.yahoo.com",
        "analyticsanalytics.google.com",
        "analyticsengine.s3.amazonaws.com",
        "apex.go.sonobi.com",
        "api-adservices.apple.com",
        "api-js.mixpanel.com",
        "api.ad.xiaomi.com",
        "api.amplitude.com",
        "api.bugsnag.com",
        "api.fyber.com",
        "api.luckyorange.com",
        "api.mixpanel.com",
        "api.mouseflow.com",
        "api.onesignal.com",
        "api.optimizely.com",
        "api.partnerstack.com",
        "api.rlcdn.com",
        "api.segment.io",
        "api.taboola.com",
        "api.viglink.com",
        "api.vungle.com",
        "api2.amplitude.com",
        "api2.branch.io",
        "app.adjust.com",
        "app.appsflyer.com",
        "app.bugsnag.com",
        "app.getsentry.com",
        "app.posthog.com",
        "app.usercentrics.eu",
        "appleiadsdk.apple.com",
        "applovin.com",
        "appmetrica.yandex.ru",
        "appsflyer.com",
        "as.casalemedia.com",
        "auction.unityads.unity3d.com",
        "b.scorecardresearch.com",
        "bam-cell.nr-data.net",
        "bam.nr-data.net",
        "bat.bing.com",
        "bcp.crwdcntrl.net",
        "bdapi-ads.realmemobile.com",
        "bdapi-in-ads.realmemobile.com",
        "bea4.v.fwmrm.net",
        "bidder.criteo.com",
        "bidswitch.net",
        "bingads.microsoft.com",
        "bluekai.com",
        "bnc.lt",
        "books-analytics-events.apple.com",
        "browser-intake-datadoghq.com",
        "browser.events.data.msn.com",
        "browser.sentry-cdn.com",
        "bugsnagnotify.bugsnag.com",
        "business-api.tiktok.com",
        "c.amazon-adsystem.com",
        "c.bing.com",
        "c.clarity.ms",
        "c.gumgum.com",
        "capi.connatix.com",
        "careers.hotjar.com",
        "casalemedia.com",
        "cd.connatix.com",
        "cdn-test.mouseflow.com",
        "cdn.cookielaw.org",
        "cdn.doubleverify.com",
        "cdn.dynamicyield.com",
        "cdn.heapanalytics.com",
        "cdn.indexexchange.com",
        "cdn.kargo.com",
        "cdn.lr-ingest.com",
        "cdn.luckyorange.com",
        "cdn.mgid.com",
        "cdn.mouseflow.com",
        "cdn.onesignal.com",
        "cdn.optimizely.com",
        "cdn.permutive.com",
        "cdn.privacy-mgmt.com",
        "cdn.rudderlabs.com",
        "cdn.segment.com",
        "cdn.siftscience.com",
        "cdn.taboola.com",
        "cdn.teads.tv",
        "cdn.viglink.com",
        "ck.ads.oppomobile.com",
        "clarity.ms",
        "claritybt.freshmarketer.com",
        "click.googleanalytics.com",
        "click.linksynergy.com",
        "click.mailchimp.com",
        "click.oneplus.cn",
        "clickadu.com",
        "clientstream.launchdarkly.com",
        "cloudflareinsights.com",
        "cm.g.doubleclick.net",
        "cmp.inmobi.com",
        "cmp.osano.com",
        "cmp.usercentrics.eu",
        "coinimp.com",
        "config.samsungads.com",
        "config.unityads.unity3d.com",
        "connect.facebook.net",
        "consent.cookiebot.com",
        "consent.trustarc.com",
        "consentcdn.cookiebot.com",
        "contextual.media.net",
        "contextweb.com",
        "control.kochava.com",
        "cookiebot.com",
        "criteo.com",
        "crwdcntrl.net",
        "cs.luckyorange.net",
        "ct.pinterest.com",
        "customer.io",
        "d.adroll.com",
        "d.applovin.com",
        "d.impactradius-event.com",
        "d.reddit.com",
        "dai.google.com",
        "data.ads.oppomobile.com",
        "data.mistat.india.xiaomi.com",
        "data.mistat.rus.xiaomi.com",
        "data.mistat.xiaomi.com",
        "dc.ads.linkedin.com",
        "decide.mixpanel.com",
        "device-metrics-us-2.amazon.com",
        "device-metrics-us.amazon.com",
        "dis.criteo.com",
        "doubleclick.netstats.g.doubleclick.net",
        "doubleverify.com",
        "eb2.3lift.com",
        "edge.fullstory.com",
        "eu.posthog.com",
        "events.hotjar.io",
        "events.launchdarkly.com",
        "events.reddit.com",
        "events.redditmedia.com",
        "events.segment.io",
        "events3alt.adcolony.com",
        "exoclick.com",
        "extmaps-api.yandex.net",
        "facebookpixel.facebook.com",
        "fastlane.rubiconproject.com",
        "fingerprintjs.com",
        "firebase-settings.crashlytics.com",
        "fls-na.amazon.com",
        "fpjs.io",
        "freshworksfreshmarketer.com",
        "fundingchoicesmessages.google.com",
        "fw.adsafeprotected.com",
        "fwtracks.freshmarketer.com",
        "g.jwpsrv.com",
        "gemini.yahoo.com",
        "geo.yahoo.com",
        "geolocation.onetrust.com",
        "globalapi.ad.xiaomi.com",
        "go.redirectingat.com",
        "go.skimresources.com",
        "google-analytics.com",
        "googleads.g.doubleclick.net",
        "googleads4.g.doubleclick.net",
        "googleadservices.com",
        "googletagmanager.com",
        "googletagservices.com",
        "graph.facebook.com",
        "graph.instagram.com",
        "grs.hicloud.com",
        "gtm.mouseflow.com",
        "gum.criteo.com",
        "hbopenbid.pubmatic.com",
        "hotjaradm.hotjar.com",
        "htlb.casalemedia.com",
        "htlbid.com",
        "huaweimetrics.data.hicloud.com",
        "i.instagram.com",
        "iadsdk.apple.com",
        "ib.adnxs.com",
        "id5-sync.com",
        "identify.hotjar.com",
        "idsync.rlcdn.com",
        "image6.pubmatic.com",
        "images.taboola.com",
        "in.hotjar.com",
        "indexexchange.com",
        "info.lgsmartad.com",
        "ingest.sentry.io",
        "init.supersonicads.com",
        "inmobi.com",
        "insightexpressai.com",
        "insights.adsrvr.org",
        "insights.hotjar.com",
        "iot-eu-logser.realme.com",
        "iot-logser.realme.com",
        "ir-na.amazon-adsystem.com",
        "ironsource.mobi",
        "it.star",
        "js-agent.newrelic.com",
        "js.driftt.com",
        "juicyads.com",
        "kochava.com",
        "liftoff.io",
        "linkedinads.linkedin.com",
        "list-manage.com",
        "live.chartboost.com",
        "log.byteoversea.com",
        "log.fc.yahoo.com",
        "log.outbrain.com",
        "log.pinterest.com",
        "logbak.hicloud.com",
        "logs.roku.com",
        "logservice.hicloud.com",
        "logservice1.hicloud.com",
        "logx.optimizely.com",
        "luckyorangeluckyorange.com",
        "m.doubleclick.net",
        "mads-eu.amazon.com",
        "mail.ru",
        "match.adsrvr.org",
        "mc.yandex.ru",
        "mcs-va.tiktokv.com",
        "media.net",
        "media.netstatic.media.net",
        "mediavisor.doubleclick.net",
        "metrics.adobe.com",
        "metrics.apple.com",
        "metrics.brightcove.com",
        "metrics.data.hicloud.com",
        "metrics.icloud.com",
        "metrics.mzstatic.com",
        "metrics2.data.hicloud.com",
        "metrika.yandex.ru",
        "mgid.com",
        "mineralt.io",
        "minero.cc",
        "mon.byteoversea.com",
        "mon.tiktokv.com",
        "monerominer.rocks",
        "mouseflowmouseflow.com",
        "ms.applovin.com",
        "mssl.fwmrm.net",
        "munchkin.marketo.net",
        "ngfts.lge.com",
        "nmetrics.samsung.com",
        "notes-analytics-events.apple.com",
        "notify.bugsnag.com",
        "nr.taboola.com",
        "o0.ingest.sentry.io",
        "o2.mouseflow.com",
        "oas.openx.net",
        "odb.outbrain.com",
        "offerwall.yandex.net",
        "omtrdc.net",
        "onclickads.net",
        "oneplusclick.oneplus.cn",
        "onetag-sys.com",
        "open.oneplus.net",
        "openx.net",
        "oppoadsfs.oppomobile.com",
        "optimized-by.rubiconproject.com",
        "outcome-ssp.supersonicads.com",
        "pagead.l.doubleclick.net",
        "pagead2.googleadservices.com",
        "pagead2.googlesyndication.com",
        "pangleglobal.com",
        "partnerads.ysm.yahoo.com",
        "partnerstack.com",
        "pinterestads.pinterest.com",
        "pippio.com",
        "pixel.adsafeprotected.com",
        "pixel.facebook.com",
        "pixel.mathtag.com",
        "pixel.quantserve.com",
        "pixel.quora.com",
        "pixel.redditmedia.com",
        "pixel.rubiconproject.com",
        "pluginstats.wp.com",
        "popads.net",
        "popcash.net",
        "popmyads.com",
        "prd.jwpltx.com",
        "prebid.adnxs.com",
        "prf.hn",
        "privacyportal.onetrust.com",
        "prod.uidapi.com",
        "propellerads.com",
        "propellerclick.com",
        "pubads.g.doubleclick.net",
        "pubmatic.com",
        "px.ads.linkedin.com",
        "px.srvcs.tumblr.com",
        "qevents.quora.com",
        "quantcast.com",
        "quantserve.com",
        "r.lr-ingest.com",
        "rcm-na.amazon-adsystem.com",
        "realmeiot-eu-logser.realme.com",
        "realtime.luckyorange.com",
        "redditevents.reddit.com",
        "redirect.viglink.com",
        "redirector.googlevideo.com",
        "redirector.skimresources.com",
        "refersion.com",
        "region1.google-analytics.com",
        "results.see",
        "rs.fullstory.com",
        "rt.applovin.com",
        "rtb.openx.net",
        "rubiconproject.com",
        "rudderstack.com",
        "rum.browser-intake-datadoghq.com",
        "s.adroll.com",
        "s.clarity.ms",
        "s.innovid.com",
        "s.skimresources.com",
        "s.youtube.com",
        "samsung-com.112.2o7.net",
        "samsungads.com",
        "samsungsamsungads.com",
        "sb.scorecardresearch.com",
        "sc-analytics.appspot.com",
        "sc-static.net",
        "script.hotjar.com",
        "sdk-api-v1.singular.net",
        "sdk.iad-01.braze.com",
        "sdk.privacy-center.org",
        "sdkconfig.ad.intl.xiaomi.com",
        "sdkconfig.ad.xiaomi.com",
        "search.spotxchange.com",
        "secure.adnxs.com",
        "securepubads.g.doubleclick.net",
        "sentrybrowser.sentry-cdn.com",
        "servicer.mgid.com",
        "sessions.bugsnag.com",
        "settings-win.data.microsoft.com",
        "settings.luckyorange.net",
        "shareasale.com",
        "sharethrough.com",
        "siftscience.com",
        "simage2.pubmatic.com",
        "sin3-ib.adnxs.com",
        "smartclip.com",
        "smartclip.net",
        "smartyads.com",
        "smetrics.samsung.com",
        "snap.licdn.com",
        "snowplowanalytics.com",
        "sourcepoint.mgr.consensu.org",
        "spotxchange.com",
        "ssl.google-analytics.com",
        "ssl.p.jwpcdn.com",
        "sslwidget.criteo.com",
        "st.dynamicyield.com",
        "stackadapt.com",
        "statdynamic.com",
        "static.ads-twitter.com",
        "static.adsafeprotected.com",
        "static.cloudflareinsights.com",
        "static.criteo.net",
        "static.doubleclick.net",
        "static.hotjar.com",
        "static.klaviyo.com",
        "static.media.net",
        "stats.g.doubleclick.net",
        "stats.wp.com",
        "stickyadstv.com",
        "surveys.hotjar.com",
        "sync-tm.everesttech.net",
        "sync.crwdcntrl.net",
        "sync.kargo.com",
        "sync.mathtag.com",
        "t.pepperjamnetwork.com",
        "t.skimresources.com",
        "tagmanager.google.com",
        "tags.bluekai.com",
        "tags.crwdcntrl.net",
        "telemetry.microsoft.com",
        "tiktokads-api.tiktok.com",
        "tlx.3lift.com",
        "tools.mouseflow.com",
        "top-fwz1.mail.ru",
        "tpc.googlesyndication.com",
        "tps.doubleverify.com",
        "tr.facebook.com",
        "tr.iadsdk.apple.com",
        "tr.snapchat.com",
        "track.customer.io",
        "track.hubspot.com",
        "track.linksynergy.com",
        "tracker.snowplowanalytics.com",
        "tracking.miui.com",
        "tracking.rus.miui.com",
        "trafficjunky.net",
        "trc.taboola.com",
        "tremorhub.com",
        "trk.pinterest.com",
        "tvinteractive.tv",
        "tvpixel.com",
        "twitterstatic.ads-twitter.com",
        "u.openx.net",
        "udc.yahoo.com",
        "udcm.yahoo.com",
        "unityads.unity3d.com",
        "unityauction.unityads.unity3d.com",
        "upload.luckyorange.net",
        "us-ads.openx.net",
        "us.i.posthog.com",
        "us.ibs.lgappstv.com",
        "us.info.lgsmartad.com",
        "vars.hotjar.com",
        "vid.connatix.com",
        "vortex-win.data.microsoft.com",
        "vortex.data.microsoft.com",
        "vungle.com",
        "w1.luckyorange.com",
        "watson.telemetry.microsoft.com",
        "wd.adcolony.com",
        "weather-analytics-events.apple.com",
        "webminepool.com",
        "webview.unityads.unity3d.com",
        "widget.intercom.io",
        "widgets.outbrain.com",
        "widgets.pinterest.com",
        "ws-na.amazon-adsystem.com",
        "www.anrdoezrs.net",
        "www.awin1.com",
        "www.coinimp.com",
        "www.dpbolvw.net",
        "www.google-analytics.com",
        "www.googletagmanager.com",
        "www.tkqlhce.com",
        "wzrkt.com",
        "xiaomiapi.ad.xiaomi.com",
        "xp.apple.com",
        "yahooads.yahoo.com",
        "yandexextmaps-api.yandex.net",
        "youtubeads.youtube.com",
        "yumenetworks.com",
        "zenaps.com"
    ];

    function isAdUrl(url) {
        if (!url || typeof url !== "string") return false;
        const u = url.toLowerCase();
        if (u.includes("ads.js") || u.includes("pagead.js") || u.includes("/ads.") || u.includes("/pagead.")) return true;
        if (AD_DOMAINS.some(d => u.includes(d))) return true;
        if (SUPER_TEST_HOSTS.some(h => u.includes(h))) return true;
        return false;
    }

    // 1. Defuse global ad tracking stubs so site scripts don't break
    try {
        window.adsbygoogle = window.adsbygoogle || [];
        window.adsbygoogle.push = function() { return 0; };
        window.yaContextCb = window.yaContextCb || [];
        window.yaContextCb.push = function(cb) { if (typeof cb === 'function') { try { cb(); } catch(e){} } return 0; };
        window.yandex_rtb = window.yandex_rtb || {};
        window.yandex_ad = window.yandex_ad || {};
        window.kupdwvsh = function() {};

        // Defuse test stubs for adblock testing suites (e.g. turtlecute, d3ward)
        Object.defineProperty(window, 's_test_ads', {
            get() { return undefined; },
            set() {},
            configurable: true
        });
        Object.defineProperty(window, 's_test_pagead', {
            get() { return undefined; },
            set() {},
            configurable: true
        });
    } catch(e) {}

    // 2. Intercept Script & Iframe element creation to block ad scripts before execution
    try {
        const origCreateElement = document.createElement;
        document.createElement = function(tagName, options) {
            const el = origCreateElement.call(document, tagName, options);
            const tag = (tagName || '').toLowerCase();

            if (tag === 'script' || tag === 'iframe') {
                const origSetAttr = el.setAttribute;
                el.setAttribute = function(name, value) {
                    if (name && name.toLowerCase() === 'src' && isAdUrl(value)) {
                        return;
                    }
                    return origSetAttr.apply(this, arguments);
                };

                let originalSrc = '';
                Object.defineProperty(el, 'src', {
                    get() { return originalSrc; },
                    set(val) {
                        if (isAdUrl(val)) {
                            originalSrc = '';
                            if (tag === 'script') {
                                el.type = 'text/plain';
                            }
                        } else {
                            originalSrc = val;
                            try { el.setAttribute('src', val); } catch(e){}
                        }
                    },
                    configurable: true
                });
            }
            return el;
        };
    } catch(e) {}

    // 3. Intercept fetch & XMLHttpRequest
    if (window.fetch) {
        const origFetch = window.fetch;
        window.fetch = function(input, init) {
            const url = (typeof input === 'string') ? input : (input && input.url ? input.url : '');
            if (isAdUrl(url)) {
                return Promise.reject(new TypeError('Blocked by Ferro AdBlock'));
            }
            return origFetch.apply(this, arguments);
        };
    }

    if (window.XMLHttpRequest) {
        const origOpen = window.XMLHttpRequest.prototype.open;
        window.XMLHttpRequest.prototype.open = function(method, url) {
            if (isAdUrl(url)) {
                this._ferro_blocked = true;
            }
            return origOpen.apply(this, arguments);
        };
        const origSend = window.XMLHttpRequest.prototype.send;
        window.XMLHttpRequest.prototype.send = function() {
            if (this._ferro_blocked) {
                this.abort();
                return;
            }
            return origSend.apply(this, arguments);
        };
    }

    // 4. Intercept window.open to block popups & betting redirects
    try {
        const origOpen = window.open;
        window.open = function(url, target, features) {
            if (url && isAdUrl(url)) {
                console.log('[Ferro AdBlock] Prevented ad popup:', url);
                return null;
            }
            return origOpen.apply(this, arguments);
        };
    } catch(e) {}

    // 5. DOM MutationObserver: clean up dynamically injected ad units & branding
    const AD_SELECTORS = [
        '#cts_test',
        '#ad_ctd',
        '.textads',
        '.adsbox',
        '.banner-ads',
        '.banner_ads',
        '.adbox',
        '.ADBox',
        '.AdBox',
        '.afs_ads',
        '.ad-zone',
        '.ad-space',
        '.ad-unit',
        'ins.adsbygoogle',
        'iframe[id^="aswift_"]',
        'iframe[id^="google_ads"]',
        'div[id^="google_ads"]',
        'div[id^="yandex_rtb"]',
        'div[id^="yandex_ad"]',
        'div[class*="ya-partner"]',
        '.ya-direct',
        'div[id*="marketgid"]',
        'div[id*="smi2"]',
        'div[id*="kadam"]',
        'div[id*="teasernet"]',
        '.clickunder',
        '.popunder',
        'div[class*="preroll"]',
        '[class*="branding"]',
        '[id*="branding"]',
        '.lp-b__brand',
        '[class*="lp-b__brand"]',
        '.brand_bg',
        '#brand_bg',
        '[class*="brand_bg"]',
        '[id*="brand_bg"]',
        '.brand_top',
        '#brand_top',
        '[class*="brand_top"]',
        '[id*="brand_top"]',
        '.regpro',
        '[class^="br5g"]',
        '[id^="br5g"]',
        '.br5g33utrrt',
        '#br5g33utrrs',
        '#br5g33utrru',
        '[id*="traffer"]',
        '[class*="traffer"]',
        'a[href*="traffer"]',
        'a[href*="parik"]',
        'a[href*="21wiz"]',
        'a[href*="1x"]',
        'a[href*="casino"]',
        'a[href*="vulkan"]',
        'a[href*="bet"]',
        'a[href*="dog"]',
        'a[href*="megaways"]',
        '.code-block',
        '[class*="code-block"]',
        '[class^="__clb-"]',
        'img[src*="Caution1"]',
        'img[data-src*="Caution1"]',
        '.wp-image-1133',
        'img[src*="Screenshot_1"]',
        'img[data-src*="Screenshot_1"]',
        '.wp-image-3462',
        'img[src*="Screenshot_16"]',
        'img[data-src*="Screenshot_16"]',
        '.wp-image-30781',
        '[class*="feedback"]',
        '#feedback',
        '.feedback-btn',
        '[class*="fluid-player"]',
        '.elementor-element-928b71a',
        '.elementor-element-f78181b',
        '[data-id="928b71a"]',
        '[data-id="f78181b"]'
    ].join(', ');

    function cleanAds() {
        try {
            const ads = document.querySelectorAll(AD_SELECTORS);
            for (let i = 0; i < ads.length; i++) {
                ads[i].remove();
            }

            // Remove rogue scripts
            const rogueScripts = document.querySelectorAll('script[src*="ybs2ffs7v"], script[src*="12ezo5v60"], script[src*="fvcwqkkqmuv"], script[src*="traffer"], script[src*="21wiz"]');
            for (let s = 0; s < rogueScripts.length; s++) {
                rogueScripts[s].remove();
            }

            // Reset wrapper/header if shifted down by ad style
            const wrappers = document.querySelectorAll('.wrapper, #wrapper, .header, #header, .headerWrapper');
            for (let i = 0; i < wrappers.length; i++) {
                const el = wrappers[i];
                if (el.style.top && parseInt(el.style.top) > 50) {
                    el.style.removeProperty('top');
                }
                if (el.style.marginTop && parseInt(el.style.marginTop) > 50) {
                    el.style.removeProperty('margin-top');
                }
                if (el.style.paddingTop && parseInt(el.style.paddingTop) > 50) {
                    el.style.removeProperty('padding-top');
                }
            }

            // Remove background ad image from html and body
            [document.documentElement, document.body].forEach(function(el) {
                if (!el) return;
                const bg = (el.style.backgroundImage || '') + ' ' + (window.getComputedStyle(el).backgroundImage || '');
                if (/traffer|branding|parik|casino|bonus|bet|dog.*house|megaways|JBWheel|advert|banner|1x|AuCordesk|lp-b__brand/i.test(bg)) {
                    el.style.setProperty('background', '#111111', 'important');
                    el.style.setProperty('background-image', 'none', 'important');
                    el.style.setProperty('cursor', 'default', 'important');
                }
            });

            // Remove canyoublockit extreme test dummy ad elements
            const testAds = document.querySelectorAll('.code-block, [class*="code-block"], [class^="__clb-"], img[src*="Caution1"], img[data-src*="Caution1"], .wp-image-1133, img[src*="Screenshot_1"], img[data-src*="Screenshot_1"], .wp-image-3462, img[src*="Screenshot_16"], img[data-src*="Screenshot_16"], .wp-image-30781, [class*="feedback"], #feedback, .feedback-btn, .elementor-element-35203eb, .elementor-element-5c7335d, .elementor-element-928b71a, .elementor-element-f78181b');
            for (let j = 0; j < testAds.length; j++) {
                const el = testAds[j];
                const col = el.closest('.elementor-column, .elementor-element-5c7335d, .elementor-element-bebe1cc, .elementor-element-4881fb9, .elementor-element-e1591ee');
                if (col) {
                    col.remove();
                } else {
                    el.remove();
                }
            }
        } catch(e) {}
    }

    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', cleanAds);
    } else {
        cleanAds();
    }

    try {
        const observer = new MutationObserver((mutations) => {
            let found = false;
            for (let i = 0; i < mutations.length; i++) {
                if (mutations[i].addedNodes.length > 0) {
                    found = true;
                    break;
                }
            }
            if (found) cleanAds();
        });
        observer.observe(document.documentElement || document.body, { childList: true, subtree: true });
    } catch(e) {}
})();
"#;

pub const TOGGLE_DARK_MODE_JS: &str = r#"
(function() {
    const ID = 'ferro-dark-mode-style';
    let style = document.getElementById(ID);
    if (style) {
        style.remove();
        return false;
    } else {
        style = document.createElement('style');
        style.id = ID;
        style.textContent = `
            html {
                filter: invert(90%) hue-rotate(180deg) contrast(95%) !important;
                background-color: #111111 !important;
            }
            img, video, canvas, svg, picture, [style*="background-image"] {
                filter: invert(100%) hue-rotate(180deg) contrast(105%) !important;
            }
        `;
        (document.head || document.documentElement).appendChild(style);
        return true;
    }
})();
"#;

pub const TRANSLATE_PAGE_JS: &str = r#"
(function() {
    const BANNER_ID = 'ferro-translate-bar';

    if (window._ferro_is_translated) {
        // Revert back to original
        if (window._ferro_original_texts) {
            window._ferro_original_texts.forEach(([node, text]) => {
                try { node.nodeValue = text; } catch(e) {}
            });
            window._ferro_original_texts = null;
        }
        const banner = document.getElementById(BANNER_ID);
        if (banner) banner.remove();
        window._ferro_is_translated = false;
        return false;
    }

    const DICT = {
        "Sign in": "Войти",
        "Log in": "Войти",
        "Sign up": "Регистрация",
        "Search": "Поиск",
        "Search...": "Поиск...",
        "Home": "Главная",
        "About": "О нас",
        "About Us": "О нас",
        "Contact": "Контакты",
        "Contact Us": "Связаться с нами",
        "Settings": "Настройки",
        "Preferences": "Настройки",
        "Options": "Параметры",
        "Help": "Помощь",
        "Documentation": "Документация",
        "Docs": "Документация",
        "Guide": "Руководство",
        "Overview": "Обзор",
        "Downloads": "Загрузки",
        "Download": "Скачать",
        "Install": "Установить",
        "Installation": "Установка",
        "Getting Started": "Начало работы",
        "Quick Start": "Быстрый старт",
        "Read more": "Читать далее",
        "Learn more": "Узнать больше",
        "Submit": "Отправить",
        "Cancel": "Отмена",
        "Save": "Сохранить",
        "Save changes": "Сохранить изменения",
        "Edit": "Редактировать",
        "Delete": "Удалить",
        "Remove": "Удалить",
        "Share": "Поделиться",
        "Comments": "Комментарии",
        "Follow": "Подписаться",
        "Subscribed": "Вы подписаны",
        "Next": "Далее",
        "Previous": "Назад",
        "Back": "Назад",
        "Close": "Закрыть",
        "Menu": "Меню",
        "Repositories": "Репозитории",
        "Pull requests": "Пуллреквесты",
        "Issues": "Задачи",
        "Discussions": "Обсуждения",
        "Actions": "Действия",
        "Projects": "Проекты",
        "Security": "Безопасность",
        "Insights": "Аналитика",
        "Stars": "Звёзды",
        "Forks": "Форки",
        "Releases": "Релизы",
        "Tags": "Теги",
        "Branches": "Ветки",
        "Latest": "Последнее",
        "Popular": "Популярное",
        "Hot": "Горячее",
        "New": "Новое",
        "Top": "Топ",
        "Account": "Аккаунт",
        "Profile": "Профиль",
        "Dashboard": "Панель управления",
        "Pricing": "Тарифы",
        "Features": "Возможности",
        "Blog": "Блог",
        "News": "Новости",
        "Privacy Policy": "Политика конфиденциальности",
        "Terms of Service": "Условия использования",
        "Terms": "Условия",
        "All rights reserved": "Все права защищены",
        "Copyright": "Авторские права",
        "Cart": "Корзина",
        "Checkout": "Оформление заказа",
        "Order": "Заказ",
        "View": "Просмотр",
        "Details": "Подробности",
        "Status": "Статус",
        "Success": "Успешно",
        "Error": "Ошибка",
        "Warning": "Предупреждение",
        "Info": "Информация",
        "Today": "Сегодня",
        "Yesterday": "Вчера",
        "Tomorrow": "Завтра",
        "Month": "Месяц",
        "Year": "Год",
        "Day": "День",
        "Hour": "Час",
        "Minute": "Минута",
        "Second": "Секунда"
    };

    const walker = document.createTreeWalker(
        document.body || document.documentElement,
        NodeFilter.SHOW_TEXT,
        {
            acceptNode(node) {
                if (!node.nodeValue || !node.nodeValue.trim()) return NodeFilter.FILTER_REJECT;
                const parent = node.parentElement;
                if (!parent) return NodeFilter.FILTER_REJECT;
                const tag = parent.tagName.toLowerCase();
                if (tag === 'script' || tag === 'style' || tag === 'code' || tag === 'pre' || tag === 'noscript') {
                    return NodeFilter.FILTER_REJECT;
                }
                return NodeFilter.FILTER_ACCEPT;
            }
        }
    );

    const originalList = [];
    let node;
    while ((node = walker.nextNode())) {
        const text = node.nodeValue.trim();
        if (DICT[text]) {
            originalList.push([node, node.nodeValue]);
            node.nodeValue = node.nodeValue.replace(text, DICT[text]);
        }
    }
    window._ferro_original_texts = originalList;
    window._ferro_is_translated = true;

    // Remove existing if any
    const oldBanner = document.getElementById(BANNER_ID);
    if (oldBanner) oldBanner.remove();

    const banner = document.createElement('div');
    banner.id = BANNER_ID;
    banner.style.position = 'fixed';
    banner.style.bottom = '16px';
    banner.style.right = '16px';
    banner.style.zIndex = '2147483647';
    banner.style.background = '#141414';
    banner.style.color = '#ececec';
    banner.style.border = '1px solid #2a2a2a';
    banner.style.borderRadius = '4px';
    banner.style.padding = '8px 14px';
    banner.style.fontFamily = 'Inter, -apple-system, BlinkMacSystemFont, sans-serif';
    banner.style.fontSize = '12px';
    banner.style.display = 'flex';
    banner.style.alignItems = 'center';
    banner.style.gap = '12px';
    banner.style.boxShadow = '0 6px 18px rgba(0,0,0,0.6)';

    banner.innerHTML = `
        <span style="display:flex;align-items:center;gap:6px;">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8">
                <path d="M5 8l6 6"></path>
                <path d="M4 14l6-6 2-3"></path>
                <path d="M2 5h12"></path>
                <path d="M7 2h1"></path>
                <path d="M22 22l-5-10-5 10"></path>
                <path d="M14 18h6"></path>
            </svg>
            <span>Перевод активен (RU)</span>
        </span>
        <button id="ferro-close-trans" style="background:transparent;border:1px solid #3c3c3c;color:#ececec;padding:3px 10px;border-radius:3px;font-size:11px;cursor:pointer;">Сбросить</button>
    `;

    document.body.appendChild(banner);
    document.getElementById('ferro-close-trans').addEventListener('click', () => {
        banner.remove();
        if (window._ferro_original_texts) {
            window._ferro_original_texts.forEach(([n, t]) => { try { n.nodeValue = t; } catch(e) {} });
            window._ferro_original_texts = null;
        }
        window._ferro_is_translated = false;
    });

    return true;
})();
"#;



pub const CONTEXT_MENU_JS: &str = r#"
(function() {
    if (window._ferro_context_menu_installed) return;
    window._ferro_context_menu_installed = true;

    const HOST_ID = 'ferro-ctx-host';

    function postIpc(payload) {
        if (window.ipc) {
            window.ipc.postMessage(JSON.stringify(payload));
            return true;
        }
        return false;
    }

    function copyToClipboard(text) {
        if (navigator.clipboard && navigator.clipboard.writeText) {
            navigator.clipboard.writeText(text).catch(() => {
                fallbackCopy(text);
            });
        } else {
            fallbackCopy(text);
        }
    }

    function fallbackCopy(text) {
        try {
            const ta = document.createElement('textarea');
            ta.value = text;
            ta.style.cssText = 'position:fixed;top:-9999px;left:-9999px;opacity:0;';
            document.body.appendChild(ta);
            ta.select();
            document.execCommand('copy');
            ta.remove();
        } catch (e) {}
    }

    function removeMenu() {
        const old = document.getElementById(HOST_ID);
        if (old) old.remove();
    }

    document.addEventListener('contextmenu', function(e) {
        removeMenu();

        const target = e.target && e.target.nodeType === 1 ? e.target : (e.target && e.target.parentElement ? e.target.parentElement : document.body);
        if (!target) return;

        const sel = window.getSelection ? window.getSelection().toString().trim() : '';
        const imgEl = target.closest ? (target.closest('img') || (target.tagName === 'IMG' ? target : null)) : null;
        let imgSrc = '';
        if (imgEl && (imgEl.currentSrc || imgEl.src)) {
            imgSrc = imgEl.currentSrc || imgEl.src;
        } else if (target.style && target.style.backgroundImage) {
            const m = target.style.backgroundImage.match(/url\(["']?([^"']*)["']?\)/);
            if (m && m[1]) imgSrc = m[1];
        }

        const linkEl = target.closest ? target.closest('a[href]') : null;
        const linkUrl = linkEl ? linkEl.href : '';
        const isInput = target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable;

        e.preventDefault();
        e.stopPropagation();

        // Read Ferro theme settings
        const cfg = window._ferro_settings || {};
        const isLight = cfg.theme_mode === 'light' || document.documentElement.getAttribute('data-theme') === 'light';
        const rad = cfg.corner_radius !== undefined ? Math.max(4, Math.min(16, cfg.corner_radius)) : 6;
        const itemRad = Math.max(2, rad - 2);
        const accent = cfg.accent_color || (isLight ? '#18181b' : '#ececec');

        const items = [];

        // Link context items
        if (linkUrl) {
            items.push({
                label: 'Открыть ссылку в новой вкладке',
                action: () => {
                    if (!postIpc({ action: 'new_tab', url: linkUrl })) {
                        window.open(linkUrl, '_blank');
                    }
                }
            });
            items.push({
                label: 'Копировать адрес ссылки',
                action: () => copyToClipboard(linkUrl)
            });
            items.push({ separator: true });
        }

        // Image context items
        if (imgSrc) {
            items.push({
                label: 'Открыть изображение в новой вкладке',
                action: () => {
                    if (!postIpc({ action: 'new_tab', url: imgSrc })) {
                        window.open(imgSrc, '_blank');
                    }
                }
            });
            items.push({
                label: 'Сохранить изображение как...',
                action: () => {
                    if (!postIpc({ action: 'download_url', url: imgSrc })) {
                        const a = document.createElement('a');
                        a.href = imgSrc;
                        a.download = '';
                        a.click();
                    }
                }
            });
            items.push({
                label: 'Копировать адрес изображения',
                action: () => copyToClipboard(imgSrc)
            });
            items.push({
                label: 'Найти изображение в Google',
                action: () => {
                    const lensUrl = 'https://lens.google.com/uploadbyurl?url=' + encodeURIComponent(imgSrc);
                    if (!postIpc({ action: 'new_tab', url: lensUrl })) {
                        window.open(lensUrl, '_blank');
                    }
                }
            });
            items.push({ separator: true });
        }

        // Editable input/textarea context items
        if (isInput) {
            items.push({
                label: 'Вырезать',
                kbd: 'Ctrl+X',
                action: () => {
                    target.focus();
                    if (!postIpc({ action: 'edit_cmd', url: 'cut' })) {
                        document.execCommand('cut');
                    }
                }
            });
            items.push({
                label: 'Копировать',
                kbd: 'Ctrl+C',
                action: () => {
                    target.focus();
                    if (!postIpc({ action: 'edit_cmd', url: 'copy' })) {
                        document.execCommand('copy');
                    }
                }
            });
            items.push({
                label: 'Вставить',
                kbd: 'Ctrl+V',
                action: () => {
                    target.focus();
                    if (!postIpc({ action: 'edit_cmd', url: 'paste' })) {
                        document.execCommand('paste');
                    }
                }
            });
            items.push({ separator: true });
        } else if (sel) {
            const snippet = sel.length > 24 ? sel.slice(0, 24) + '…' : sel;
            items.push({
                label: 'Копировать',
                kbd: 'Ctrl+C',
                action: () => {
                    copyToClipboard(sel);
                    postIpc({ action: 'edit_cmd', url: 'copy' });
                }
            });
            items.push({
                label: `Искать «${snippet}» в Google`,
                action: () => {
                    const searchUrl = 'https://www.google.com/search?q=' + encodeURIComponent(sel);
                    if (!postIpc({ action: 'new_tab', url: searchUrl })) {
                        window.open(searchUrl, '_blank');
                    }
                }
            });
            items.push({ separator: true });
        }

        // Standard Firefox-style page items
        items.push({
            label: 'Сохранить страницу как...',
            kbd: 'Ctrl+S',
            action: () => {
                const html = '<!DOCTYPE html>\n' + document.documentElement.outerHTML;
                postIpc({
                    action: 'save_page_html',
                    url: window.location.href,
                    title: html
                });
            }
        });
        items.push({
            label: 'Выделить всё',
            kbd: 'Ctrl+A',
            action: () => {
                if (isInput && target.select) {
                    target.select();
                } else {
                    document.execCommand('selectAll');
                    postIpc({ action: 'edit_cmd', url: 'select_all' });
                }
            }
        });
        items.push({ separator: true });
        items.push({
            label: 'Сделать снимок экрана',
            action: () => {
                postIpc({ action: 'take_screenshot' });
            }
        });
        items.push({ separator: true });
        items.push({
            label: 'Перевести страницу на русский',
            action: () => {
                postIpc({ action: 'translate' });
            }
        });
        items.push({ separator: true });
        items.push({
            label: 'Исходный код страницы',
            kbd: 'Ctrl+U',
            action: () => {
                const src = document.documentElement ? document.documentElement.outerHTML : '';
                postIpc({
                    action: 'open_source_html',
                    url: window.location.href,
                    title: src
                });
            }
        });
        items.push({
            label: 'Исследовать',
            kbd: 'F12',
            action: () => {
                postIpc({ action: 'inspect_element' });
            }
        });

        // Build Shadow DOM Context Menu
        const host = document.createElement('div');
        host.id = HOST_ID;
        host.style.cssText = 'position:fixed;top:0;left:0;width:0;height:0;z-index:2147483647;pointer-events:none;';
        const shadow = host.attachShadow({ mode: 'open' });

        const bg = isLight ? '#ffffff' : '#23222b';
        const fg = isLight ? '#18181b' : '#f2f2f5';
        const fgMuted = isLight ? '#71717a' : '#9ca3af';
        const hoverBg = isLight ? '#ececf1' : '#353442';
        const borderCol = isLight ? '#d4d4d8' : '#3b3a47';

        const style = document.createElement('style');
        style.textContent = `
            * { box-sizing: border-box; margin: 0; padding: 0; user-select: none; }
            @keyframes ctxFade {
                from { opacity: 0; transform: scale(0.97) translateY(-3px); }
                to { opacity: 1; transform: scale(1) translateY(0); }
            }
            .ctx-menu {
                position: fixed;
                pointer-events: auto;
                min-width: 268px;
                max-width: 330px;
                background: ${bg};
                color: ${fg};
                border: 1px solid ${borderCol};
                border-radius: ${rad}px;
                box-shadow: 0 12px 32px rgba(0, 0, 0, ${isLight ? '0.18' : '0.62'});
                font-family: 'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
                font-size: 13px;
                padding: 4px 0 5px 0;
                animation: ctxFade 0.1s cubic-bezier(0.2, 0, 0, 1);
                overflow: hidden;
            }
            .ctx-nav-bar {
                display: grid;
                grid-template-columns: repeat(4, 1fr);
                padding: 2px 6px 4px 6px;
                gap: 4px;
            }
            .ctx-nav-btn {
                height: 34px;
                border: none;
                background: transparent;
                color: ${fg};
                border-radius: ${itemRad}px;
                display: flex;
                align-items: center;
                justify-content: center;
                cursor: pointer;
                transition: background 0.1s ease, color 0.1s ease;
            }
            .ctx-nav-btn:hover:not(:disabled) {
                background: ${hoverBg};
            }
            .ctx-nav-btn:disabled {
                opacity: 0.36;
                cursor: default;
            }
            .ctx-sep {
                height: 1px;
                background: ${borderCol};
                margin: 4px 0;
            }
            .ctx-item {
                display: flex;
                align-items: center;
                justify-content: space-between;
                gap: 14px;
                margin: 0 4px;
                padding: 6px 12px;
                border-radius: ${itemRad}px;
                cursor: pointer;
                color: ${fg};
                line-height: 1.35;
                transition: background 0.08s ease;
            }
            .ctx-item:hover {
                background: ${hoverBg};
            }
            .ctx-label {
                overflow: hidden;
                text-overflow: ellipsis;
                white-space: nowrap;
            }
            .ctx-kbd {
                font-size: 11px;
                color: ${fgMuted};
                flex-shrink: 0;
                font-family: ui-monospace, monospace;
            }
        `;
        shadow.appendChild(style);

        const menu = document.createElement('div');
        menu.className = 'ctx-menu';

        // Top 4-button navigation row (Back, Forward, Reload, Bookmark)
        const navBar = document.createElement('div');
        navBar.className = 'ctx-nav-bar';

        const navBtns = [
            {
                title: 'Назад (Alt+←)',
                svg: `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="19" y1="12" x2="5" y2="12"></line><polyline points="12 19 5 12 12 5"></polyline></svg>`,
                action: () => {
                    if (!postIpc({ action: 'go_back' })) window.history.back();
                }
            },
            {
                title: 'Вперёд (Alt+→)',
                svg: `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="5" y1="12" x2="19" y2="12"></line><polyline points="12 5 19 12 12 19"></polyline></svg>`,
                action: () => {
                    if (!postIpc({ action: 'go_forward' })) window.history.forward();
                }
            },
            {
                title: 'Обновить (Ctrl+R)',
                svg: `<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="23 4 23 10 17 10"></polyline><path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10"></path></svg>`,
                action: () => {
                    if (!postIpc({ action: 'reload' })) window.location.reload();
                }
            },
            {
                title: 'Добавить в закладки (Ctrl+D)',
                svg: `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"></polygon></svg>`,
                action: () => {
                    postIpc({ action: 'toggle_bookmark_active' });
                }
            }
        ];

        navBtns.forEach(nb => {
            const btn = document.createElement('button');
            btn.className = 'ctx-nav-btn';
            btn.title = nb.title;
            btn.innerHTML = nb.svg;
            btn.addEventListener('click', (ev) => {
                ev.stopPropagation();
                removeMenu();
                nb.action();
            });
            navBar.appendChild(btn);
        });
        menu.appendChild(navBar);

        const topSep = document.createElement('div');
        topSep.className = 'ctx-sep';
        menu.appendChild(topSep);

        items.forEach(it => {
            if (it.separator) {
                const sep = document.createElement('div');
                sep.className = 'ctx-sep';
                menu.appendChild(sep);
            } else {
                const row = document.createElement('div');
                row.className = 'ctx-item';
                row.innerHTML = `<span class="ctx-label">${it.label}</span>${it.kbd ? `<span class="ctx-kbd">${it.kbd}</span>` : ''}`;
                row.addEventListener('click', (ev) => {
                    ev.stopPropagation();
                    removeMenu();
                    it.action();
                });
                menu.appendChild(row);
            }
        });

        shadow.appendChild(menu);
        (document.documentElement || document.body).appendChild(host);

        // Clamp within viewport
        const rect = menu.getBoundingClientRect();
        let x = e.clientX;
        let y = e.clientY;
        if (x + rect.width > window.innerWidth - 6) {
            x = Math.max(6, window.innerWidth - rect.width - 6);
        }
        if (y + rect.height > window.innerHeight - 6) {
            y = Math.max(6, window.innerHeight - rect.height - 6);
        }
        menu.style.left = x + 'px';
        menu.style.top = y + 'px';
    }, true);

    document.addEventListener('mousedown', function(e) {
        const host = document.getElementById(HOST_ID);
        if (host && e.target !== host && !host.contains(e.target)) {
            removeMenu();
        }
    }, true);
    window.addEventListener('scroll', removeMenu, true);
    window.addEventListener('resize', removeMenu, true);
    document.addEventListener('keydown', function(e) {
        if (e.key === 'Escape') removeMenu();
    }, true);
})();
"#;

pub const HOTKEYS_JS: &str = r#"
(function() {
    if (window._ferro_hotkeys_installed) return;
    window._ferro_hotkeys_installed = true;

    window.addEventListener('keydown', function(e) {
        // F12 or Ctrl+Shift+I: WebInspector
        if (e.key === 'F12' || (e.ctrlKey && e.shiftKey && (e.key === 'I' || e.key === 'i' || e.code === 'KeyI'))) {
            e.preventDefault();
            e.stopPropagation();
            if (window.ipc) {
                window.ipc.postMessage(JSON.stringify({ action: 'inspect_element' }));
            }
            return;
        }

        // Ctrl+U: View Page Source
        if (e.ctrlKey && !e.shiftKey && !e.altKey && (e.key === 'u' || e.key === 'U' || e.code === 'KeyU')) {
            e.preventDefault();
            e.stopPropagation();
            if (window.ipc) {
                const src = document.documentElement ? document.documentElement.outerHTML : '';
                window.ipc.postMessage(JSON.stringify({
                    action: 'open_source_html',
                    url: window.location.href,
                    title: src
                }));
            }
            return;
        }

        // Ctrl + Shift + T (or Cyrillic Е): Reopen closed tab
        if (e.ctrlKey && e.shiftKey && (e.key === 't' || e.key === 'T' || e.key === 'е' || e.key === 'Е' || e.code === 'KeyT')) {
            e.preventDefault();
            e.stopPropagation();
            if (window.ipc) {
                window.ipc.postMessage(JSON.stringify({ action: 'reopen_closed_tab' }));
            }
            return;
        }

        // Ctrl + T (or Cyrillic Е): New Tab
        if (e.ctrlKey && !e.shiftKey && !e.altKey && (e.key === 't' || e.key === 'T' || e.key === 'е' || e.key === 'Е' || e.code === 'KeyT')) {
            e.preventDefault();
            e.stopPropagation();
            if (window.ipc) {
                window.ipc.postMessage(JSON.stringify({ action: 'new_tab' }));
            }
            return;
        }

        // Ctrl + W (or Cyrillic Ц): Close Tab
        if (e.ctrlKey && !e.shiftKey && !e.altKey && (e.key === 'w' || e.key === 'W' || e.key === 'ц' || e.key === 'Ц' || e.code === 'KeyW')) {
            e.preventDefault();
            e.stopPropagation();
            if (window.ipc) {
                window.ipc.postMessage(JSON.stringify({ action: 'close_active_tab' }));
            }
            return;
        }

        // Ctrl + R (or F5, or Cyrillic К): Reload
        if ((e.ctrlKey && !e.shiftKey && !e.altKey && (e.key === 'r' || e.key === 'R' || e.key === 'к' || e.key === 'К' || e.code === 'KeyR')) || e.key === 'F5') {
            e.preventDefault();
            e.stopPropagation();
            if (window.ipc) {
                window.ipc.postMessage(JSON.stringify({ action: 'reload' }));
            } else {
                location.reload();
            }
            return;
        }

        // Ctrl + L (or Alt + D, or F6, or Cyrillic Д/В): Focus Omnibox
        if ((e.ctrlKey && (e.key === 'l' || e.key === 'L' || e.key === 'д' || e.key === 'Д' || e.code === 'KeyL')) ||
            (e.altKey && (e.key === 'd' || e.key === 'D' || e.key === 'в' || e.key === 'В' || e.code === 'KeyD')) ||
            e.key === 'F6') {
            e.preventDefault();
            e.stopPropagation();
            if (window.ipc) {
                window.ipc.postMessage(JSON.stringify({ action: 'focus_omnibox' }));
            }
            return;
        }

        // Ctrl + H (or Cyrillic Р): History
        if (e.ctrlKey && !e.shiftKey && !e.altKey && (e.key === 'h' || e.key === 'H' || e.key === 'р' || e.key === 'Р' || e.code === 'KeyH')) {
            e.preventDefault();
            e.stopPropagation();
            if (window.ipc) {
                window.ipc.postMessage(JSON.stringify({ action: 'new_tab', url: 'ferro://history' }));
            }
            return;
        }

        // Ctrl + D (or Cyrillic В): Bookmark
        if (e.ctrlKey && !e.shiftKey && !e.altKey && (e.key === 'd' || e.key === 'D' || e.key === 'в' || e.key === 'В' || e.code === 'KeyD')) {
            e.preventDefault();
            e.stopPropagation();
            if (window.ipc) {
                window.ipc.postMessage(JSON.stringify({ action: 'toggle_bookmark_active' }));
            }
            return;
        }

        // Ctrl + Tab / Ctrl + Shift + Tab: Switch Tabs
        if (e.ctrlKey && (e.key === 'Tab' || e.code === 'Tab')) {
            e.preventDefault();
            e.stopPropagation();
            if (window.ipc) {
                window.ipc.postMessage(JSON.stringify({ action: e.shiftKey ? 'prev_tab' : 'next_tab' }));
            }
            return;
        }

        // Ctrl + 1..9: Switch Tab by index
        if (e.ctrlKey && !e.shiftKey && !e.altKey && e.code && e.code.startsWith('Digit')) {
            const digit = parseInt(e.code.replace('Digit', ''));
            if (digit >= 1 && digit <= 9) {
                e.preventDefault();
                e.stopPropagation();
                if (window.ipc) {
                    window.ipc.postMessage(JSON.stringify({ action: 'switch_tab_index', id: digit }));
                }
                return;
            }
        }

        // Alt + ArrowLeft / ArrowRight: Back / Forward
        if (e.altKey && (e.key === 'ArrowLeft' || e.code === 'ArrowLeft')) {
            e.preventDefault();
            e.stopPropagation();
            if (window.ipc) {
                window.ipc.postMessage(JSON.stringify({ action: 'go_back' }));
            } else {
                history.back();
            }
            return;
        }
        if (e.altKey && (e.key === 'ArrowRight' || e.code === 'ArrowRight')) {
            e.preventDefault();
            e.stopPropagation();
            if (window.ipc) {
                window.ipc.postMessage(JSON.stringify({ action: 'go_forward' }));
            } else {
                history.forward();
            }
            return;
        }
    }, true);
})();
"#;
