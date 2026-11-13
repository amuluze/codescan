from django.contrib.auth.decorators import login_required
from django.http import HttpResponse


# ruleid: py/example-django-view-unprotected
def delete_post(request, post_id):
    return HttpResponse("deleted")


# ruleid: py/example-django-view-unprotected
def destroy_session(request):
    return HttpResponse("ok")


@login_required
def remove_comment(request, comment_id):
    return HttpResponse("removed")


def delete_items(items):
    return list(items)
