---
layout: default
title: Blog
permalink: /blog/
---

# Blog

Thoughts, updates, and deep dives into Claude Code observability.

<ul class="post-list">
{% for post in site.posts %}
  <li class="post-list-item">
    <h3 class="post-list-title">
      <a href="{{ post.url | relative_url }}">{{ post.title }}</a>
    </h3>
    <p class="post-list-meta">
      {{ post.date | date: "%B %-d, %Y" }}
      {% if post.tags %}
        &bull;
        {% for tag in post.tags %}
          <span class="tag tag-{{ tag | slugify }}">{{ tag }}</span>
        {% endfor %}
      {% endif %}
    </p>
    {% if post.excerpt %}
      <p class="post-list-excerpt">{{ post.excerpt | strip_html | truncate: 200 }}</p>
    {% endif %}
  </li>
{% endfor %}
</ul>

{% if site.posts.size == 0 %}
<p class="text-muted">No posts yet. Check back soon!</p>
{% endif %}
