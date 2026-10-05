function escapeHtml(s) {
var d = document.createElement('div');
d.textContent = s;
return d.innerHTML;
}
function showError(el, msg) {
el.innerHTML = '<div class="flash-message flash-error">' + escapeHtml(msg) + '</div>';
}
(function() {
var path = window.location.pathname;
var parentMap = {
'/build/': '/builds',
'/project/': '/projects',
'/jobset/': '/projects',
'/evaluation/': '/evaluations',
'/channel/': '/channels',
};
var effectivePath = path;
for (var prefix in parentMap) {
if (path.startsWith(prefix)) {
effectivePath = parentMap[prefix];
break;
}
}
var links = document.querySelectorAll('.nav-links a');
for (var i = 0; i < links.length; i++) {
var href = links[i].getAttribute('href');
if (href === '/' ? path === '/' : effectivePath === href || path === href || path.startsWith(href + '/')) {
links[i].classList.add('active');
}
}
})();
// Tick any element carrying data-live-elapsed="<unix-epoch-seconds>" once
// per second so running build timers advance without a page refresh.
(function() {
function fmt(secs) {
if (secs < 0) secs = 0;
var m = Math.floor(secs / 60);
var s = secs % 60;
return m > 0 ? m + 'm ' + s + 's' : s + 's';
}
function tick() {
var now = Math.floor(Date.now() / 1000);
var nodes = document.querySelectorAll('[data-live-elapsed]');
for (var i = 0; i < nodes.length; i++) {
var start = parseInt(nodes[i].getAttribute('data-live-elapsed'), 10);
if (!isNaN(start)) nodes[i].textContent = fmt(now - start);
}
}
tick();
setInterval(tick, 1000);
})();
// Poll running evaluations so their attribute counts advance, and reload once
// one finishes so its builds show up.
(function() {
function count(n) { return n.toLocaleString('en-US'); }
function poll(node) {
var id = node.getAttribute('data-eval-progress');
fetch('/api/v1/evaluations/' + id, {credentials: 'same-origin'})
.then(function(response) { return response.ok ? response.json() : Promise.reject(); })
.then(function(evaluation) {
if (evaluation.status !== 'running') return location.reload();
var done = evaluation.attrs_done;
var total = evaluation.attrs_total;
if (done != null && total) {
node.querySelector('.eval-progress-count').textContent = count(done) + ' / ' + count(total);
var percent = Math.min(100, Math.floor(done * 100 / total));
node.querySelector('.eval-progress-bar > span').style.width = percent + '%';
}
setTimeout(function() { poll(node); }, 3000);
})
.catch(function() {});
}
var nodes = document.querySelectorAll('[data-eval-progress]');
for (var i = 0; i < nodes.length; i++) setTimeout(poll, 3000, nodes[i]);
})();
// Render <time datetime> in the viewer's locale and timezone.
(function() {
var year = new Date().getFullYear();
var nodes = document.querySelectorAll('time[datetime]');
for (var i = 0; i < nodes.length; i++) {
var at = new Date(nodes[i].getAttribute('datetime'));
if (isNaN(at)) continue;
var opts = {month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit'};
if (at.getFullYear() !== year) opts.year = 'numeric';
if (nodes[i].hasAttribute('data-zone')) opts.timeZoneName = 'short';
nodes[i].textContent = at.toLocaleString(undefined, opts);
nodes[i].title = at.toLocaleString(undefined, {dateStyle: 'full', timeStyle: 'long'});
}
})();
// Send people back to the page they were on once they sign in.
(function() {
if (location.pathname === '/login') return;
var next = encodeURIComponent(location.pathname + location.search);
var links = document.querySelectorAll('a[href="/login"]');
for (var i = 0; i < links.length; i++) links[i].href = '/login?next=' + next;
})();
