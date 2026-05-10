"""
OperatorAuthBackend — Django authentication backend for S4.8.

Strategy (Option B from the story): keep Django's built-in auth.User for
session machinery, but use Operator as the source-of-truth for passwords.

Flow
----
1.  Receive (email, raw_password) from authenticate().
2.  Look up the active Operator by email.
3.  Call Operator.check_password() which delegates to Django's PASSWORD_HASHERS
    (BCryptSHA256PasswordHasher is first — see settings.py).
4.  On success, get-or-create a matching auth.User (email as username) and
    return it.  Django's session machinery then uses this User object.

The auth.User is a thin shadow — it holds no sensitive data.  The canonical
password hash lives on Operator.password only.

Sprint-5 follow-up
------------------
Replace this backend with a SAML/OIDC backend.  The Operator.password column
becomes unused for SSO-provisioned operators and can be dropped in a follow-up
migration once all tenants have migrated.
"""

from django.contrib.auth import get_user_model
from django.contrib.auth.backends import BaseBackend

User = get_user_model()


class OperatorAuthBackend(BaseBackend):
    """Authenticate against the Operator table's bcrypt-hashed password."""

    def authenticate(self, request, username=None, password=None, **kwargs):
        # Accept either positional username= or explicit email= kwarg.
        email = username or kwargs.get("email")
        if not email or not password:
            return None

        # Local import avoids circular imports at module load time.
        from operators.models import Operator  # noqa: PLC0415

        try:
            operator = Operator.objects.get(email=email, is_active=True)
        except Operator.DoesNotExist:
            # Run a dummy check to mitigate timing attacks even on miss.
            Operator().check_password(password)
            return None

        if not operator.check_password(password):
            return None

        # get-or-create the shadow auth.User — never store a password on it.
        user, created = User.objects.get_or_create(
            username=email,
            defaults={"email": email, "is_active": True},
        )
        if created:
            # Unusable password so nobody can log in via ModelBackend.
            user.set_unusable_password()
            user.save(update_fields=["password"])

        return user

    def get_user(self, user_id):
        try:
            return User.objects.get(pk=user_id)
        except User.DoesNotExist:
            return None
