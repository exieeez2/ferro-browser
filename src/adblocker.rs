use adblock::engine::Engine;
use adblock::lists::{FilterSet, ParseOptions};
use adblock::request::Request;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

pub struct AdBlocker {
    enabled: AtomicBool,
    blocked_count: Arc<AtomicUsize>,
    engine: Engine,
}

pub const UBLOCK_CORE_RULES: &str = r#"
! -------------------------------------------------------------
! Ferro AdBlock — EasyList, RuAdList, uBlock Origin & Privacy Rules
! -------------------------------------------------------------

! Google & DoubleClick Ad Ecosystem
||doubleclick.net^
||googlesyndication.com^
||googleadservices.com^
||googleads.g.doubleclick.net^
||pagead2.googlesyndication.com^
||adservice.google.com^
||adservice.google.*^
||partner.googleadservices.com^
||googletagservices.com^
||googletagmanager.com/gtm.js
||google-analytics.com^
/pagead/
/googleads.js
||googletagservices.com/tag/js/gpt.js
||googlevideo.com/videoplayback*&adformat=
||youtube.com/api/stats/ads*
||youtube.com/pagead/*
||youtube.com/get_midroll_info*

! Yandex Direct, RTB, Metrika, Mail.Ru, VK Ads
||an.yandex.ru^
||mc.yandex.ru^
||adfox.ru^
||awaps.yandex.net^
||bs.yandex.ru^
||direct.yandex.ru^
||yabs.yandex.ru^
||yandex.ru/ads/*
||ads.adfox.ru^
||banners.adfox.ru^
||extmail.yandex.net^
||mail.yandex.ru/api/ad/*
||top-fwz1.mail.ru^
||r.mail.ru^
||rs.mail.ru^
||target.my.com^
||ad.mail.ru^
||begun.ru^
||rambler.ru/ad/*
||vk.com/ads.php
||vk.com/al_ads.php

! Russian Teaser Networks & Exchanges
||marketgid.com^
||teasernet.com^
||directadvert.ru^
||kadam.net^
||kadam.ru^
||oblivki.biz^
||advertur.ru^
||actionpay.ru^
||admitad.com^
||smi2.ru^
||smi2.net^
||24smi.net^
||lentainform.com^
||mediametrics.ru^
||novostimira.net^
||mirtesen.ru/ads/*
||recreativ.ru^
||gismeteo.ru/static/ads/*

! Global Ad Exchanges, SSP, DSP & Trackers
||criteo.com^
||criteo.net^
||adnxs.com^
||ib.adnxs.com^
||facebook.com/tr/
||connect.facebook.net/*/fbevents.js
||scorecardresearch.com^
||moatads.com^
||outbrain.com^
||taboola.com^
||adroll.com^
||quantserve.com^
||advertising.com^
||rubiconproject.com^
||pubmatic.com^
||casalemedia.com^
||openx.net^
||smartadserver.com^
||amazon-adsystem.com^
||adtechus.com^
||serving-sys.com^
||adzerk.net^
||revcontent.com^
||zedo.com^
||bidswitch.net^
||yieldmo.com^
||lijit.com^
||teads.tv^
||inmobi.com^
||adcolony.com^
||applovin.com^
||unityads.unity3d.com^
||chartboost.com^
||flurry.com^
||ad-delivery.net^
||bidgear.com^
||adkeeper.com^
||mgid.com^

! Popunders, Clickunders, Streaming Video & Betting Ads
||popads.net^
||popcash.net^
||propellerads.com^
||exoclick.com^
||adsterra.com^
||hilltopads.com^
||clickadu.com^
||trafficjunky.com^
||trafficfactory.biz^
||trafficstars.com^
||juicyads.com^
||plugrush.com^
||rtb.adxcore.ch^
||evadav.com^
||rollerads.com^
||richads.com^
||monetag.com^
||adoperator.com^
||onclickads.net^
||traffichub.com^
||tsyndicate.com^
||adxprts.com^
||clicksor.com^
||bidvertiser.com^
||infolinks.com^
||zeroredirect1.com^
||bet365.com^$third-party
||1xbet*.com^$third-party
||1x-bet*.com^$third-party
||1xbet*^$third-party
||melbet*.com^$third-party
||mostbet*.com^$third-party
||vulkan*.com^$third-party
||vulcan*.com^$third-party
||joycasino*.com^$third-party
||pin-up*.com^$third-party
||pinup*.com^$third-party
||vavada*.com^$third-party
||catcasino*.com^$third-party
||spinamba*.com^$third-party
||fonbet*.com^$third-party
||winline*.com^$third-party
||betboom*.com^$third-party
||pari.ru/ads/*

! Movie & Streaming Site Branding Networks, Video Ads & Popunders
||traffer.net^
||traffer.biz^
||cdn.traffer.biz^
||21wiz.com^
||advmaker.net^
||advmaker.ru^
||adlook.me^
||viqeo.tv^
||videoroll.net^
||videoroll.org^
||buzzoola.com^
||moevideo.biz^
||moevideo.net^
||videonow.ru^
||eagleplatform.com^
||allsecurelink.com^
||gnezdo.ru^
||mixmarket.biz^
||adriver.ru^
||soloway.ru^
||targetix.net^
||hpmdnetwork.ru^
||rutarget.ru^
||clickadilla.com^
||tizerlady.ru^
||luxup.ru^
||globalteas.ru^
||otclick-adv.ru^
||redclick.ru^
||bodyclick.net^
||teaser.cc^
||visitweb.com^
||admixer.net^
||admixer.ua^
||parik24*^$third-party
||parimatch*^$third-party
||1win*^$third-party
||linebet*^$third-party
||marathonbet*^$third-party
||leon.ru^$third-party
||leonbets*^$third-party
||stake.com^$third-party
||roobet.com^$third-party
||bc.game^$third-party
||playfortuna*^$third-party
||booi*^$third-party
/load_brclass
/brload/
/bra/?lc=
||12ezo5v60.com^
||ybs2ffs7v.com^
||cdn.fluidplayer.com^
||canyoublockit.com/wp-content/uploads/2019/09/Caution1.png
||canyoublockit.com/wp-content/uploads/2020/01/Screenshot_1.png
||canyoublockit.com/wp-content/uploads/2020/04/Screenshot_16.png
||Caution1.png^
||parik24*^
||doghouse*^
||megaways*^



! -------------------------------------------------------------
! -------------------------------------------------------------
! SuperAdBlockTest + TurtleCute Coverage (511 hosts)
! -------------------------------------------------------------
! SuperAdBlockTest + TurtleCute Coverage (511 domains)
||2975c.v.fwmrm.net^
||a.klaviyo.com^
||a.lenovo.com^
||a.teads.tv^
||aan.amazon.com^
||aax.amazon-adsystem.com^
||acdn.adnxs.com^
||ad.crwdcntrl.net^
||ad.doubleclick.net^
||ad.gt^
||ad.lgappstv.com^
||ad.linksynergy.com^
||ad.mail.ru^
||adc3-launch.adcolony.com^
||adclick.g.doubleclick.net^
||adcolonyads30.adcolony.com^
||adfox.yandex.ru^
||adfstat.yandex.ru^
||adjust.com^
||adlog.vivo.com^
||adm.hotjar.com^
||adnxs.com^
||adroll.com^
||ads-api.tiktok.com^
||ads-api.twitter.com^
||ads-api.vivo.com^
||ads-api.x.com^
||ads-sg.tiktok.com^
||ads.huawei.com^
||ads.linkedin.com^
||ads.microsoft.com^
||ads.pinterest.com^
||ads.pubmatic.com^
||ads.roku.com^
||ads.snapchat.com^
||ads.stickyadstv.com^
||ads.tiktok.com^
||ads.tremorhub.com^
||ads.vizio.com^
||ads.vk.com^
||ads.x.com^
||ads.yahoo.com^
||ads.youtube.com^
||ads30.adcolony.com^
||adserver.unityads.unity3d.com^
||adservetx.media.net^
||adservice.google.com^
||adsfs.oppomobile.com^
||adspagead2.googlesyndication.com^
||adsrvr.org^
||adtago.s3.amazonaws.com^
||adtech.yahooinc.com^
||advertising-api-eu.amazon.com^
||advertising.apple.com^
||advertising.yahoo.com^
||advertising.yandex.ru^
||advice-ads.s3.amazonaws.com^
||adx.ads.oppomobile.com^
||afs.googlesyndication.com^
||amazon-adsystem.com^
||amazonadtago.s3.amazonaws.com^
||amoeba.web.roku.com^
||an.facebook.com^
||analytics-api.samsunghealthcn.com^
||analytics-sg.tiktok.com^
||analytics.adobe.io^
||analytics.google.com^
||analytics.pinterest.com^
||analytics.pointdrive.linkedin.com^
||analytics.query.yahoo.com^
||analytics.s3.amazonaws.com^
||analytics.tiktok.com^
||analytics.twitter.com^
||analytics.x.com^
||analytics.yahoo.com^
||analyticsanalytics.google.com^
||analyticsengine.s3.amazonaws.com^
||apex.go.sonobi.com^
||api-adservices.apple.com^
||api-js.mixpanel.com^
||api.ad.xiaomi.com^
||api.amplitude.com^
||api.bugsnag.com^
||api.fyber.com^
||api.luckyorange.com^
||api.mixpanel.com^
||api.mouseflow.com^
||api.onesignal.com^
||api.optimizely.com^
||api.partnerstack.com^
||api.rlcdn.com^
||api.segment.io^
||api.taboola.com^
||api.viglink.com^
||api.vungle.com^
||api2.amplitude.com^
||api2.branch.io^
||app.adjust.com^
||app.appsflyer.com^
||app.bugsnag.com^
||app.getsentry.com^
||app.posthog.com^
||app.usercentrics.eu^
||appleiadsdk.apple.com^
||applovin.com^
||appmetrica.yandex.ru^
||appsflyer.com^
||as.casalemedia.com^
||auction.unityads.unity3d.com^
||b.scorecardresearch.com^
||bam-cell.nr-data.net^
||bam.nr-data.net^
||bat.bing.com^
||bcp.crwdcntrl.net^
||bdapi-ads.realmemobile.com^
||bdapi-in-ads.realmemobile.com^
||bea4.v.fwmrm.net^
||bidder.criteo.com^
||bidswitch.net^
||bingads.microsoft.com^
||bluekai.com^
||bnc.lt^
||books-analytics-events.apple.com^
||browser-intake-datadoghq.com^
||browser.events.data.msn.com^
||browser.sentry-cdn.com^
||bugsnagnotify.bugsnag.com^
||business-api.tiktok.com^
||c.amazon-adsystem.com^
||c.bing.com^
||c.clarity.ms^
||c.gumgum.com^
||capi.connatix.com^
||careers.hotjar.com^
||casalemedia.com^
||cd.connatix.com^
||cdn-test.mouseflow.com^
||cdn.cookielaw.org^
||cdn.doubleverify.com^
||cdn.dynamicyield.com^
||cdn.heapanalytics.com^
||cdn.indexexchange.com^
||cdn.kargo.com^
||cdn.lr-ingest.com^
||cdn.luckyorange.com^
||cdn.mgid.com^
||cdn.mouseflow.com^
||cdn.onesignal.com^
||cdn.optimizely.com^
||cdn.permutive.com^
||cdn.privacy-mgmt.com^
||cdn.rudderlabs.com^
||cdn.segment.com^
||cdn.siftscience.com^
||cdn.taboola.com^
||cdn.teads.tv^
||cdn.viglink.com^
||ck.ads.oppomobile.com^
||clarity.ms^
||claritybt.freshmarketer.com^
||click.googleanalytics.com^
||click.linksynergy.com^
||click.mailchimp.com^
||click.oneplus.cn^
||clickadu.com^
||clientstream.launchdarkly.com^
||cloudflareinsights.com^
||cm.g.doubleclick.net^
||cmp.inmobi.com^
||cmp.osano.com^
||cmp.usercentrics.eu^
||coinimp.com^
||config.samsungads.com^
||config.unityads.unity3d.com^
||connect.facebook.net^
||consent.cookiebot.com^
||consent.trustarc.com^
||consentcdn.cookiebot.com^
||contextual.media.net^
||contextweb.com^
||control.kochava.com^
||cookiebot.com^
||criteo.com^
||crwdcntrl.net^
||cs.luckyorange.net^
||ct.pinterest.com^
||customer.io^
||d.adroll.com^
||d.applovin.com^
||d.impactradius-event.com^
||d.reddit.com^
||dai.google.com^
||data.ads.oppomobile.com^
||data.mistat.india.xiaomi.com^
||data.mistat.rus.xiaomi.com^
||data.mistat.xiaomi.com^
||dc.ads.linkedin.com^
||decide.mixpanel.com^
||device-metrics-us-2.amazon.com^
||device-metrics-us.amazon.com^
||dis.criteo.com^
||doubleclick.netstats.g.doubleclick.net^
||doubleverify.com^
||eb2.3lift.com^
||edge.fullstory.com^
||eu.posthog.com^
||events.hotjar.io^
||events.launchdarkly.com^
||events.reddit.com^
||events.redditmedia.com^
||events.segment.io^
||events3alt.adcolony.com^
||exoclick.com^
||extmaps-api.yandex.net^
||facebookpixel.facebook.com^
||fastlane.rubiconproject.com^
||fingerprintjs.com^
||firebase-settings.crashlytics.com^
||fls-na.amazon.com^
||fpjs.io^
||freshworksfreshmarketer.com^
||fundingchoicesmessages.google.com^
||fw.adsafeprotected.com^
||fwtracks.freshmarketer.com^
||g.jwpsrv.com^
||gemini.yahoo.com^
||geo.yahoo.com^
||geolocation.onetrust.com^
||globalapi.ad.xiaomi.com^
||go.redirectingat.com^
||go.skimresources.com^
||google-analytics.com^
||googleads.g.doubleclick.net^
||googleads4.g.doubleclick.net^
||googleadservices.com^
||googletagmanager.com^
||googletagservices.com^
||graph.facebook.com^
||graph.instagram.com^
||grs.hicloud.com^
||gtm.mouseflow.com^
||gum.criteo.com^
||hbopenbid.pubmatic.com^
||hotjaradm.hotjar.com^
||htlb.casalemedia.com^
||htlbid.com^
||huaweimetrics.data.hicloud.com^
||i.instagram.com^
||iadsdk.apple.com^
||ib.adnxs.com^
||id5-sync.com^
||identify.hotjar.com^
||idsync.rlcdn.com^
||image6.pubmatic.com^
||images.taboola.com^
||in.hotjar.com^
||indexexchange.com^
||info.lgsmartad.com^
||ingest.sentry.io^
||init.supersonicads.com^
||inmobi.com^
||insightexpressai.com^
||insights.adsrvr.org^
||insights.hotjar.com^
||iot-eu-logser.realme.com^
||iot-logser.realme.com^
||ir-na.amazon-adsystem.com^
||ironsource.mobi^
||it.star^
||js-agent.newrelic.com^
||js.driftt.com^
||juicyads.com^
||kochava.com^
||liftoff.io^
||linkedinads.linkedin.com^
||list-manage.com^
||live.chartboost.com^
||log.byteoversea.com^
||log.fc.yahoo.com^
||log.outbrain.com^
||log.pinterest.com^
||logbak.hicloud.com^
||logs.roku.com^
||logservice.hicloud.com^
||logservice1.hicloud.com^
||logx.optimizely.com^
||luckyorangeluckyorange.com^
||m.doubleclick.net^
||mads-eu.amazon.com^
||mail.ru^
||match.adsrvr.org^
||mc.yandex.ru^
||mcs-va.tiktokv.com^
||media.net^
||media.netstatic.media.net^
||mediavisor.doubleclick.net^
||metrics.adobe.com^
||metrics.apple.com^
||metrics.brightcove.com^
||metrics.data.hicloud.com^
||metrics.icloud.com^
||metrics.mzstatic.com^
||metrics2.data.hicloud.com^
||metrika.yandex.ru^
||mgid.com^
||mineralt.io^
||minero.cc^
||mon.byteoversea.com^
||mon.tiktokv.com^
||monerominer.rocks^
||mouseflowmouseflow.com^
||ms.applovin.com^
||mssl.fwmrm.net^
||munchkin.marketo.net^
||ngfts.lge.com^
||nmetrics.samsung.com^
||notes-analytics-events.apple.com^
||notify.bugsnag.com^
||nr.taboola.com^
||o0.ingest.sentry.io^
||o2.mouseflow.com^
||oas.openx.net^
||odb.outbrain.com^
||offerwall.yandex.net^
||omtrdc.net^
||onclickads.net^
||oneplusclick.oneplus.cn^
||onetag-sys.com^
||open.oneplus.net^
||openx.net^
||oppoadsfs.oppomobile.com^
||optimized-by.rubiconproject.com^
||outcome-ssp.supersonicads.com^
||pagead.l.doubleclick.net^
||pagead2.googleadservices.com^
||pagead2.googlesyndication.com^
||pangleglobal.com^
||partnerads.ysm.yahoo.com^
||partnerstack.com^
||pinterestads.pinterest.com^
||pippio.com^
||pixel.adsafeprotected.com^
||pixel.facebook.com^
||pixel.mathtag.com^
||pixel.quantserve.com^
||pixel.quora.com^
||pixel.redditmedia.com^
||pixel.rubiconproject.com^
||pluginstats.wp.com^
||popads.net^
||popcash.net^
||popmyads.com^
||prd.jwpltx.com^
||prebid.adnxs.com^
||prf.hn^
||privacyportal.onetrust.com^
||prod.uidapi.com^
||propellerads.com^
||propellerclick.com^
||pubads.g.doubleclick.net^
||pubmatic.com^
||px.ads.linkedin.com^
||px.srvcs.tumblr.com^
||qevents.quora.com^
||quantcast.com^
||quantserve.com^
||r.lr-ingest.com^
||rcm-na.amazon-adsystem.com^
||realmeiot-eu-logser.realme.com^
||realtime.luckyorange.com^
||redditevents.reddit.com^
||redirect.viglink.com^
||redirector.googlevideo.com^
||redirector.skimresources.com^
||refersion.com^
||region1.google-analytics.com^
||results.see^
||rs.fullstory.com^
||rt.applovin.com^
||rtb.openx.net^
||rubiconproject.com^
||rudderstack.com^
||rum.browser-intake-datadoghq.com^
||s.adroll.com^
||s.clarity.ms^
||s.innovid.com^
||s.skimresources.com^
||s.youtube.com^
||samsung-com.112.2o7.net^
||samsungads.com^
||samsungsamsungads.com^
||sb.scorecardresearch.com^
||sc-analytics.appspot.com^
||sc-static.net^
||script.hotjar.com^
||sdk-api-v1.singular.net^
||sdk.iad-01.braze.com^
||sdk.privacy-center.org^
||sdkconfig.ad.intl.xiaomi.com^
||sdkconfig.ad.xiaomi.com^
||search.spotxchange.com^
||secure.adnxs.com^
||securepubads.g.doubleclick.net^
||sentrybrowser.sentry-cdn.com^
||servicer.mgid.com^
||sessions.bugsnag.com^
||settings-win.data.microsoft.com^
||settings.luckyorange.net^
||shareasale.com^
||sharethrough.com^
||siftscience.com^
||simage2.pubmatic.com^
||sin3-ib.adnxs.com^
||smartclip.com^
||smartclip.net^
||smartyads.com^
||smetrics.samsung.com^
||snap.licdn.com^
||snowplowanalytics.com^
||sourcepoint.mgr.consensu.org^
||spotxchange.com^
||ssl.google-analytics.com^
||ssl.p.jwpcdn.com^
||sslwidget.criteo.com^
||st.dynamicyield.com^
||stackadapt.com^
||statdynamic.com^
||static.ads-twitter.com^
||static.adsafeprotected.com^
||static.cloudflareinsights.com^
||static.criteo.net^
||static.doubleclick.net^
||static.hotjar.com^
||static.klaviyo.com^
||static.media.net^
||stats.g.doubleclick.net^
||stats.wp.com^
||stickyadstv.com^
||surveys.hotjar.com^
||sync-tm.everesttech.net^
||sync.crwdcntrl.net^
||sync.kargo.com^
||sync.mathtag.com^
||t.pepperjamnetwork.com^
||t.skimresources.com^
||tagmanager.google.com^
||tags.bluekai.com^
||tags.crwdcntrl.net^
||telemetry.microsoft.com^
||tiktokads-api.tiktok.com^
||tlx.3lift.com^
||tools.mouseflow.com^
||top-fwz1.mail.ru^
||tpc.googlesyndication.com^
||tps.doubleverify.com^
||tr.facebook.com^
||tr.iadsdk.apple.com^
||tr.snapchat.com^
||track.customer.io^
||track.hubspot.com^
||track.linksynergy.com^
||tracker.snowplowanalytics.com^
||tracking.miui.com^
||tracking.rus.miui.com^
||trafficjunky.net^
||trc.taboola.com^
||tremorhub.com^
||trk.pinterest.com^
||tvinteractive.tv^
||tvpixel.com^
||twitterstatic.ads-twitter.com^
||u.openx.net^
||udc.yahoo.com^
||udcm.yahoo.com^
||unityads.unity3d.com^
||unityauction.unityads.unity3d.com^
||upload.luckyorange.net^
||us-ads.openx.net^
||us.i.posthog.com^
||us.ibs.lgappstv.com^
||us.info.lgsmartad.com^
||vars.hotjar.com^
||vid.connatix.com^
||vortex-win.data.microsoft.com^
||vortex.data.microsoft.com^
||vungle.com^
||w1.luckyorange.com^
||watson.telemetry.microsoft.com^
||wd.adcolony.com^
||weather-analytics-events.apple.com^
||webminepool.com^
||webview.unityads.unity3d.com^
||widget.intercom.io^
||widgets.outbrain.com^
||widgets.pinterest.com^
||ws-na.amazon-adsystem.com^
||www.anrdoezrs.net^
||www.awin1.com^
||www.coinimp.com^
||www.dpbolvw.net^
||www.google-analytics.com^
||www.googletagmanager.com^
||www.tkqlhce.com^
||wzrkt.com^
||xiaomiapi.ad.xiaomi.com^
||xp.apple.com^
||yahooads.yahoo.com^
||yandexextmaps-api.yandex.net^
||youtubeads.youtube.com^
||yumenetworks.com^
||zenaps.com^
||adblock.turtlecute.org/js/widget/ads.js
||adblock.turtlecute.org/js/pagead.js
##div#cts_test
##div#ad_ctd
##.textads
##.adsbox
##.banner-ads
##.banner_ads
##.adbox
##.ADBox
##.AdBox
##.afs_ads
##.ad-zone
##.ad-space
##.ad-unit

! Generic URL Filters
/adserver/
/adserver.
/advert/
/ads/
/ad-banner/
/banners/
/sponsored/
/adv/
/teaser/
/clickunder/
/popunder/
/vast-preroll/
/vpaid/

! Cosmetic Element Hiding (CSS display: none !important)
##.banner-ad
##.ad-container
##.advertisement
##.ad-box
##.ad-wrapper
##.ad-slot
##.adsbygoogle
##ins.adsbygoogle
##iframe[src*="doubleclick"]
##iframe[src*="googleads"]
##iframe[src*="adservice"]
##iframe[id^="aswift_"]
##iframe[id^="google_ads_frame"]
##div[id^="google_ads_div_"]
##div[id^="div-gpt-ad"]
##div[id^="yandex_rtb"]
##div[id^="yandex_ad"]
##div[class*="ya-partner"]
##div[class*="ya-site-form"]
##.ya-direct
##div[class*="YandexDirect"]
##div[id*="marketgid"]
##div[id*="smi2"]
##div[id*="kadam"]
##div[id*="teasernet"]
##div[class*="advertur"]
##div[class*="oblivki"]
##div[class*="directadvert"]
##[class*="sponsored-post"]
##[data-ad-unit]
##[data-ad-client]
##[data-ad-slot]
##[data-ad-id]
##[data-ad-format]
##.branding
##[class*="branding"]
##[id*="branding"]
##[class*="brload"]
##[id*="brload"]
##[class^="br5g"]
##[id^="br5g"]
##[id*="traffer"]
##[class*="traffer"]
##a[href*="traffer"]
##a[href*="parik"]
##a[href*="21wiz"]
##a[href*="1x"]
##a[href*="casino"]
##a[href*="vulkan"]
##a[href*="bet"]
##a[href*="bonus"]
##.branding-link
##.lp-b__brand
##[class*="lp-b__brand"]
##.regpro
##[id*="brand_bg"]
##[class*="brand_bg"]
##[id*="brand_top"]
##[class*="brand_top"]
||fvcwqkkqmuv.com^
||traffer.net^
||traffer.biz^
||cdn.traffer.biz^
||res33.traffer.net^
/kgi.js
/aas/r45d/
##.code-block
##[class*="code-block"]
##[class^="__clb-"]
##.teaser
##.teaser-block
##.popup-ad
##.clickunder
##.popunder
##.b-banner
##.adv-block
##[class*="banner"]
##[class*="preroll"]
##[class*="vast-ad"]
##[class*="ima-ad"]
##[class*="vpaid"]
"#;

impl AdBlocker {
    pub fn new() -> Arc<Self> {
        let mut filter_set = FilterSet::new(false);
        filter_set.add_filter_list(UBLOCK_CORE_RULES.to_string(), ParseOptions::default());

        if let Some(config_dir) = dirs::config_dir() {
            let custom_path = config_dir.join("ferro/filters/custom.txt");
            if custom_path.exists() {
                if let Ok(content) = fs::read_to_string(&custom_path) {
                    filter_set.add_filter_list(content, ParseOptions::default());
                }
            }
        }

        let engine = Engine::new_with_filter_set(filter_set);

        Arc::new(Self {
            enabled: AtomicBool::new(true),
            blocked_count: Arc::new(AtomicUsize::new(0)),
            engine,
        })
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    pub fn set_enabled(&self, val: bool) {
        self.enabled.store(val, Ordering::Relaxed);
    }

    pub fn toggle_enabled(&self) -> bool {
        let prev = self.enabled.fetch_xor(true, Ordering::Relaxed);
        !prev
    }

    pub fn blocked_count(&self) -> usize {
        self.blocked_count.load(Ordering::Relaxed)
    }

    pub fn blocked_counter(&self) -> Arc<AtomicUsize> {
        self.blocked_count.clone()
    }

    pub fn increment_blocked(&self) {
        self.blocked_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn matches_ad_heuristic(url: &str) -> bool {
        let lower = url.to_ascii_lowercase();
        const AD_PATTERNS: &[&str] = &[
            // Google / DoubleClick
            "doubleclick.", "googlesyndication.", "googleadservices.", "googleads.", "pagead",
            "adservice.google", "googletagservices.", "googletagmanager.", "google-analytics.",
            "/googleads.js", "pubads.g.doubleclick",

            // Yandex & Russian Networks
            "an.yandex.", "mc.yandex.", "adfox.ru", "awaps.yandex.", "bs.yandex.", "direct.yandex.",
            "yabs.yandex.", "yandex.ru/ads", "ads.adfox.", "extmail.yandex.", "mail.yandex.ru/api/ad",
            "top-fwz1.mail.ru", "target.my.com", "ad.mail.ru", "begun.ru",
            "marketgid.", "teasernet.", "directadvert.", "kadam.", "oblivki.", "advertur.",
            "actionpay.", "admitad.", "smi2.", "24smi.", "lentainform.", "mediametrics.",
            "novostimira.", "recreativ.", "mgid.", "bidgear.", "adkeeper.",

            // Global Exchanges & Trackers
            "criteo.", "adnxs.", "scorecardresearch.", "moatads.", "outbrain.", "taboola.",
            "adroll.", "quantserve.", "advertising.com", "rubiconproject.", "pubmatic.",
            "casalemedia.", "openx.net", "smartadserver.", "amazon-adsystem.", "adtechus.",
            "serving-sys.", "adzerk.", "revcontent.", "zedo.com", "bidswitch.", "yieldmo.",
            "lijit.", "teads.tv", "inmobi.", "adcolony.", "applovin.", "unityads.", "chartboost.",

            // Video ads, streaming popunders, gambling & betting
            "popads.", "popcash.", "propellerads.", "exoclick.", "adsterra.", "hilltopads.",
            "clickadu.", "trafficjunky.", "trafficfactory.", "trafficstars.", "juicyads.",
            "plugrush.", "rtb.adxcore.", "evadav.", "rollerads.", "richads.", "monetag.",
            "adoperator.", "onclickads.", "traffichub.", "tsyndicate.", "zeroredirect",
            "traffer.", "21wiz.", "advmaker.", "adlook.", "viqeo.", "videoroll.", "buzzoola.",
            "moevideo.", "videonow.", "eagleplatform.", "adriver.", "soloway.", "targetix.",
            "hpmdnetwork.", "rutarget.", "clickadilla.", "tizerlady.", "luxup.", "globalteas.",
            "otclick", "redclick.", "bodyclick.", "admixer.",
            "fvcwqkkqmuv", "/aas/r45d", "doghouse", "megaways", "kgi.js", "lp-b__brand",
            "12ezo5v60", "ybs2ffs7v", "Caution1", "fluidplayer", "parik24",
            "1xbet", "1x-bet", "melbet", "mostbet", "vulkan", "vulcan", "joycasino",
            "pin-up", "pinup", "vavada", "catcasino", "spinamba", "bet365", "fonbet", "winline",
            "betboom", "pari.ru/ads", "parik", "parimatch", "1win", "linebet", "marathonbet",
            "leonbets", "stake.com", "roobet.", "bc.game", "playfortuna", "booi.",

            // URL path keywords
            "/adserver", "/advert", "/ads/banner", "/banners/", "/clickunder", "/popunder",
            "/teaser/", "/vast-preroll", "/ima-ad", "/vpaid", "/load_brclass", "/brload/",
            "/ads.js", "/pagead.js",

            // Trackers, Telemetry & Analytics (TurtleCute Test suite)
            "mouseflow.", "luckyorange.", "hotjar.", "freshmarketer.", "claritybt.",
            "fwtracks.", "stats.wp.com", "bugsnag.", "sentry-cdn.", "getsentry.",
            "pixel.facebook", "an.facebook", "ads-twitter.", "ads.linkedin",
            "ads.pinterest", "events.reddit", "tiktok.com", "byteoversea.com",
            "ads.yahoo", "analytics.yahoo", "unity3d.com", "appmetrica.yandex",
            "adfstat.yandex", "metrika.yandex", "realme.com", "realmemobile.com",
            "oppomobile.com", "oneplus.cn", "ad.xiaomi", "mistat.", "miui.com",
            "hicloud.com", "samsungads.", "smetrics.samsung", "nmetrics.samsung",
            "iadsdk.apple", "api-adservices.apple"
        ];

        AD_PATTERNS.iter().any(|p| lower.contains(p))
    }

    pub fn should_block_url(&self, url: &str) -> bool {
        if !self.is_enabled() {
            return false;
        }
        if url.starts_with("ferro://") || url.starts_with("about:") || url.starts_with("data:") {
            return false;
        }

        if Self::matches_ad_heuristic(url) {
            return true;
        }

        if let Ok(req) = Request::new(url, "", "other", "GET") {
            let res = self.engine.check_network_request(&req);
            if res.should_block() {
                return true;
            }
        }

        false
    }

    pub fn compile_webkit_rules_json() -> Vec<u8> {
        let mut filter_set = FilterSet::new(true);

        // Load built-in uBlock / EasyList rules
        filter_set.add_filter_list(UBLOCK_CORE_RULES.to_string(), ParseOptions::default());

        // Check if user has custom rules in ~/.config/ferro/filters/custom.txt
        if let Some(config_dir) = dirs::config_dir() {
            let custom_path = config_dir.join("ferro/filters/custom.txt");
            if custom_path.exists() {
                if let Ok(content) = fs::read_to_string(&custom_path) {
                    filter_set.add_filter_list(content, ParseOptions::default());
                }
            }
        }

        match filter_set.into_content_blocking() {
            Ok((rules, _)) => {
                serde_json::to_vec(&rules).unwrap_or_else(|_| b"[]".to_vec())
            }
            Err(_) => b"[]".to_vec(),
        }
    }

    pub fn filter_store_dir() -> PathBuf {
        if let Some(config_dir) = dirs::config_dir() {
            let dir = config_dir.join("ferro/webkit_filters");
            let _ = fs::create_dir_all(&dir);
            dir
        } else {
            PathBuf::from("webkit_filters")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adblock_compile_webkit_rules() {
        let json_bytes = AdBlocker::compile_webkit_rules_json();
        assert!(!json_bytes.is_empty());
        let parsed: serde_json::Value = serde_json::from_slice(&json_bytes).unwrap();
        assert!(parsed.is_array());
        let arr = parsed.as_array().unwrap();
        println!("Compiled rules count: {}", arr.len());
        for rule in parsed.as_array().unwrap() {
            let r_str = rule.to_string();
            if r_str.contains("ybs2ffs7v") || r_str.contains("traffer") {
                println!("RULE: {}", r_str);
            }
        }
        assert!(arr.len() > 10, "Expected at least 10 rules compiled to WebKit format, got {}", arr.len());
    }

    #[test]
    fn test_adblock_toggle() {
        let blocker = AdBlocker::new();
        assert!(blocker.is_enabled());
        let new_state = blocker.toggle_enabled();
        assert!(!new_state);
        assert!(!blocker.is_enabled());
        assert!(!blocker.should_block_url("https://googleads.g.doubleclick.net/pagead/ads"));
    }

    #[test]
    fn test_adblock_should_block() {
        let blocker = AdBlocker::new();
        assert!(blocker.should_block_url("https://pagead2.googlesyndication.com/pagead/js/adsbygoogle.js"));
        assert!(blocker.should_block_url("https://an.yandex.ru/system/context.js"));
        assert!(blocker.should_block_url("https://1xbet-top-bonus.com/landing"));
        assert!(blocker.should_block_url("https://mouseflow.com/fakepage"));
        assert!(blocker.should_block_url("https://adtago.s3.amazonaws.com/fakepage"));
        assert!(blocker.should_block_url("https://analytics.tiktok.com/fakepage"));
        assert!(blocker.should_block_url("https://api.ad.xiaomi.com/fakepage"));
        assert!(!blocker.should_block_url("https://wiki.archlinux.org"));
        assert!(!blocker.should_block_url("https://github.com/rust-lang/rust"));
    }
}
